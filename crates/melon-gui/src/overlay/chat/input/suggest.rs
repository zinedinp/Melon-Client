//! `CommandSuggestions`.

use super::*;

/// What completing a command draws on.
pub(super) struct Completer<'a> {
    pub(super) tree: &'a CommandTree,
    pub(super) source: Source<'a>,
    pub(super) lang: &'a Lang,
}

impl<'a> Completer<'a> {
    /// Copies the shared parts out of `ctx`, leaving it free to change.
    pub(super) fn new(ctx: &OverlayContext<'a>, players: &'a [String]) -> Self {
        Self {
            tree: ctx.commands,
            source: Source {
                registries: ctx.registries,
                players,
                selectors: ctx.permission_level >= SELECTOR_LEVEL,
            },
            lang: ctx.lang,
        }
    }
}

/// The completion list above the input (`CommandSuggestions.SuggestionsList`).
pub(super) struct SuggestionList {
    /// The input as it was when the list came, which each pick rewrites.
    pub(super) original: String,
    pub(super) start: usize,
    pub(super) length: usize,
    pub(super) matches: Vec<(String, Option<StyledText>)>,
    pub(super) selected: usize,
    pub(super) offset: usize,
    /// `tabCycles`: Tab moves on once one suggestion was taken.
    pub(super) cycled: bool,
}

impl ChatInput {
    /// `updateCommandInfo` without the completions.
    pub(super) fn update_command_info(&mut self, completer: &Completer) {
        self.usage.clear();
        let value = self.value();
        if !value.starts_with('/') {
            self.colors = None;
            return;
        }
        let tree = completer.tree;
        let parse = tree.complete(&value[1..]);
        let char_at = |byte: usize| value[..1 + byte].chars().count();
        let mut colors = vec![colors::GRAY; value.chars().count()];
        let mut next = 0;
        for span in &parse.spans {
            let color = match span.kind {
                SpanKind::Literal => continue,
                SpanKind::Argument => {
                    let color = ARGUMENT_COLORS[next % ARGUMENT_COLORS.len()];
                    next += 1;
                    color
                }
                SpanKind::Unparsed => colors::RED,
            };
            for c in &mut colors[char_at(span.range.start)..char_at(span.range.end)] {
                *c = color;
            }
        }
        self.colors = Some((value.clone(), colors));

        let before = self.input.before_cursor();
        let at = tree.complete(&before[1..]);
        self.usage_start = char_at(at.start);
        let at_end = self.input.cursor() == self.input.len();
        if at_end && at.unparsed && at.literals.is_empty() && at.arguments.is_empty() {
            let lang = completer.lang;
            let position = 1 + parse.spans.last().map_or(0, |s| s.range.start);
            let chars: Vec<char> = value.chars().collect();
            let cursor = char_at(position - 1).min(chars.len());
            let mut context = String::new();
            if cursor > 10 {
                context.push_str("...");
            }
            context.extend(&chars[cursor.saturating_sub(10)..cursor]);
            context.push_str("<--[HERE]");
            let message = lang.format(
                "command.context.parse_error",
                &[
                    lang.get("command.unknown.argument"),
                    &position.to_string(),
                    &context,
                ],
            );
            self.usage.push(StyledText::plain(&message));
            return;
        }
        let gray = TextStyle::colored(colors::GRAY);
        for line in tree.smart_usage(at.parent) {
            let mut text = StyledText::default();
            text.push(&line, &gray);
            self.usage.push(text);
        }
    }

    /// `onEdited`: completions are asked for again.
    pub(super) fn edited(
        &mut self,
        settings: &ChatSettings,
        completer: &Completer,
    ) -> OverlayAction {
        self.is_draft = false;
        self.allow_suggestions = true;
        self.suggestions = None;
        self.request(settings, false, completer)
    }

    /// `updateCommandInfo`.
    pub(super) fn request(
        &mut self,
        settings: &ChatSettings,
        forced: bool,
        completer: &Completer,
    ) -> OverlayAction {
        let text = self.input.before_cursor();
        if !text.starts_with('/') || !(settings.auto_suggestions || forced) {
            return OverlayAction::None;
        }
        let completion = completer.tree.complete(&text[1..]);
        let word = &text[1 + completion.start..];
        let start = text[..1 + completion.start].chars().count();
        let mut matches: Vec<(String, Option<StyledText>)> =
            completion.literals.into_iter().map(|m| (m, None)).collect();
        let mut ask_server = false;
        for argument in &completion.arguments {
            match command_suggestions::suggest(argument, word, &completer.source) {
                Some(found) => matches.extend(found.into_iter().map(|s| {
                    let tooltip = s
                        .tooltip
                        .map(|key| StyledText::plain(completer.lang.get_dynamic(key)));
                    (s.text, tooltip)
                })),
                None => ask_server = true,
            }
        }
        matches.sort_by_cached_key(|(m, _)| m.to_lowercase());
        matches.dedup_by(|a, b| a.0 == b.0);
        if !matches.is_empty() && !(matches.len() == 1 && matches[0].0 == word && !forced) {
            self.suggestions = Some(SuggestionList {
                original: self.value(),
                start,
                length: word.chars().count(),
                matches,
                selected: 0,
                offset: 0,
                cycled: false,
            });
            return OverlayAction::None;
        }
        if !ask_server {
            return OverlayAction::None;
        }
        self.transaction += 1;
        OverlayAction::RequestSuggestions {
            transaction: self.transaction,
            text,
        }
    }

    /// `ClientboundCommandSuggestionsPacket` for the latest request.
    pub(super) fn receive_suggestions(&mut self, suggestions: Suggestions, lang: &Lang) {
        if suggestions.transaction != self.transaction || !self.allow_suggestions {
            return;
        }
        if suggestions.matches.is_empty() {
            self.suggestions = None;
            return;
        }
        let value = self.value();
        let start = suggestions.start.min(self.input.len());
        self.suggestions = Some(SuggestionList {
            original: value,
            start,
            length: suggestions.length,
            matches: suggestions
                .matches
                .into_iter()
                .map(|(text, tooltip)| {
                    (text, tooltip.map(|t| StyledText::from_component(&t, lang)))
                })
                .collect(),
            selected: 0,
            offset: 0,
            cycled: false,
        });
    }

    /// Player names for the word before the cursor.
    pub(super) fn suggest_names(&mut self, players: &[String]) {
        let before = self.input.before_cursor();
        let start = before.rfind(' ').map_or(0, |i| i + 1);
        let word = before[start..].to_lowercase();
        let mut matches: Vec<(String, Option<StyledText>)> = players
            .iter()
            .filter(|name| name.to_lowercase().starts_with(&word))
            .map(|name| (name.clone(), None))
            .collect();
        matches.sort_by(|a, b| a.0.cmp(&b.0));
        if matches.is_empty() {
            return;
        }
        self.suggestions = Some(SuggestionList {
            original: self.value(),
            start: before[..start].chars().count(),
            length: word.chars().count(),
            matches,
            selected: 0,
            offset: 0,
            cycled: false,
        });
    }

    /// `SuggestionsList.useSuggestion`.
    pub(super) fn use_suggestion(&mut self) {
        let Some(list) = &mut self.suggestions else {
            return;
        };
        let original: Vec<char> = list.original.chars().collect();
        let start = list.start.min(original.len());
        let end = (list.start + list.length).min(original.len());
        let picked = &list.matches[list.selected].0;
        let mut value: String = original[..start].iter().collect();
        value.push_str(picked);
        value.extend(&original[end..]);
        let cursor = start + picked.chars().count();
        list.cycled = true;
        self.input.set_value(&value);
        self.input.set_cursor(cursor);
    }

    pub(super) fn cycle(&mut self, step: isize) {
        if let Some(list) = &mut self.suggestions {
            let count = list.matches.len() as isize;
            list.selected = (list.selected as isize + step).rem_euclid(count) as usize;
            if list.selected < list.offset {
                list.offset = list.selected;
            } else if list.selected >= list.offset + SUGGESTION_LINES {
                list.offset = list.selected + 1 - SUGGESTION_LINES;
            }
        }
    }

    /// Escape: closes the completion list first. `true` when it did.
    pub(super) fn close_suggestions(&mut self) -> bool {
        self.suggestions.take().is_some()
    }
}
