//! `ClientboundCommandsPacket`.

use anyhow::{Result, bail};

use crate::network::protocol::packet::Reader;
use crate::network::protocol::reader::initial_capacity;

pub const CB_COMMANDS: i32 = pumpkin_data::packet::clientbound::play::COMMANDS.0;

const KIND_MASK: u8 = 3;
const EXECUTABLE: u8 = 4;
const HAS_REDIRECT: u8 = 8;
const HAS_SUGGESTION_TYPE: u8 = 16;

/// What an argument's parser (`ArgumentTypeInfos`) means for completing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parser {
    /// `brigadier:bool`.
    Bool,
    /// `entity`, and whether it is limited to players.
    Entity { players_only: bool },
    /// `game_profile`.
    GameProfile,
    /// `gamemode`.
    GameMode,
    /// `resource_or_tag`, `resource_or_tag_key`, `resource` and the other registry parsers.
    Registry { registry: Box<str>, tags: bool },
    /// Anything else, by its id.
    Other(i32),
}

/// An argument node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Argument {
    pub name: Box<str>,
    pub parser: Parser,
    /// The parser's `ArgumentTypeInfos` id.
    pub parser_id: i32,
    /// Takes the rest of the line (a greedy string or a message).
    pub greedy: bool,
    /// The words it spans otherwise.
    pub words: usize,
    /// The node's custom suggestion provider (`minecraft:ask_server`, ...).
    pub suggestions: Option<Box<str>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Root,
    Literal(Box<str>),
    Argument(Argument),
}

/// How a part of the command parsed (`CommandSuggestions.formatText`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanKind {
    Literal,
    Argument,
    /// Fits nothing the command tree allows.
    Unparsed,
}

/// A part of the command, in bytes into it (no slash).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: std::ops::Range<usize>,
    pub kind: SpanKind,
}

/// What may complete the last word of a command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Completion {
    /// Where the word starts, in bytes into the command (no slash).
    pub start: usize,
    /// The node whose children may stand there (`SuggestionContext.parent`).
    pub parent: usize,
    /// How the command parsed, part by part.
    pub spans: Vec<Span>,
    /// Some part fits nothing: the command can't be right.
    pub unparsed: bool,
    /// The literals it may become, sorted.
    pub literals: Vec<String>,
    /// The arguments that may stand there.
    pub arguments: Vec<Argument>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: NodeKind,
    /// Runs on its own (`getCommand() != null`): what follows is optional.
    pub executable: bool,
    pub children: Vec<usize>,
    pub redirect: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct CommandTree {
    pub nodes: Vec<Node>,
    pub root: usize,
}

/// Reads an argument's properties (`ArgumentTypeInfo.serializeToNetwork`).
fn read_parser(r: &mut Reader, parser: i32) -> Result<(Parser, bool)> {
    let mut greedy = false;
    let mut kind = Parser::Other(parser);
    match parser {
        0 => kind = Parser::Bool,
        1..=4 => {
            let flags = r.u8()?;
            let size = match parser {
                1 | 3 => 4,
                _ => 8,
            };
            let count = usize::from(flags & 1) + usize::from((flags >> 1) & 1);
            r.skip(size * count)?;
        }
        5 => greedy = r.var_int()? == 2,
        6 => {
            let flags = r.u8()?;
            kind = Parser::Entity {
                players_only: flags & 2 != 0,
            };
        }
        7 => kind = Parser::GameProfile,
        42 => kind = Parser::GameMode,
        31 => r.skip(1)?,
        43 => r.skip(4)?,
        44..=48 => {
            kind = Parser::Registry {
                registry: r.string()?.into(),
                tags: matches!(parser, 44 | 45),
            };
        }
        20 => greedy = true,
        _ => {}
    }
    Ok((kind, greedy))
}

/// The command's words with their byte offsets.
pub fn split_words(command: &str) -> Vec<(usize, &str)> {
    let mut words = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in command.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' if depth > 0 || i == start => quote = Some(c),
            '[' | '{' | '(' => depth += 1,
            ']' | '}' | ')' => depth = depth.saturating_sub(1),
            ' ' if depth == 0 => {
                words.push((start, &command[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    words.push((start, &command[start..]));
    words
}

/// `ClientboundCommandsPacket.validateEntries`: every index in range, the
/// redirects and then the children buildable in some order (no cycles).
fn validate(nodes: &[Node]) -> Result<()> {
    let in_range = |i: &usize| *i < nodes.len();
    if !nodes
        .iter()
        .all(|n| n.children.iter().all(in_range) && n.redirect.iter().all(in_range))
    {
        bail!("command node index out of range");
    }
    let built = |n: &Node, left: &[bool]| n.redirect.is_none_or(|r| !left[r]);
    let resolved = |n: &Node, left: &[bool]| n.children.iter().all(|&c| !left[c]);
    for ready in [&built as &dyn Fn(&Node, &[bool]) -> bool, &resolved] {
        let mut left = vec![true; nodes.len()];
        let mut remaining = nodes.len();
        while remaining > 0 {
            let before = remaining;
            for i in 0..nodes.len() {
                if left[i] && ready(&nodes[i], &left) {
                    left[i] = false;
                    remaining -= 1;
                }
            }
            if remaining == before {
                bail!("server sent an impossible command tree");
            }
        }
    }
    Ok(())
}

impl CommandTree {
    pub fn decode(body: &[u8]) -> Result<Self> {
        let mut r = Reader::new(body);
        let count = r.list_count()?;
        let mut nodes = Vec::with_capacity(initial_capacity(count));
        for _ in 0..count {
            let flags = r.u8()?;
            let children = (0..r.var_int_array_count()?)
                .map(|_| r.var_int().map(|c| c as usize))
                .collect::<Result<Vec<_>>>()?;
            let redirect = if flags & HAS_REDIRECT != 0 {
                Some(r.var_int()? as usize)
            } else {
                None
            };
            let kind = match flags & KIND_MASK {
                0 => NodeKind::Root,
                1 => NodeKind::Literal(r.string()?.into()),
                2 => {
                    let name = r.string()?.into();
                    let parser_id = r.var_int()?;
                    let (parser, greedy) = read_parser(&mut r, parser_id)?;
                    let suggestions = if flags & HAS_SUGGESTION_TYPE != 0 {
                        Some(r.string()?.into())
                    } else {
                        None
                    };
                    let words = match parser {
                        Parser::Other(8 | 10) => 3,
                        Parser::Other(9 | 11 | 29) => 2,
                        _ => 1,
                    };
                    NodeKind::Argument(Argument {
                        name,
                        parser,
                        parser_id,
                        greedy,
                        words,
                        suggestions,
                    })
                }
                _ => bail!("unknown command node kind"),
            };
            nodes.push(Node {
                kind,
                executable: flags & EXECUTABLE != 0,
                children,
                redirect,
            });
        }
        let root = r.var_int()? as usize;
        if root >= nodes.len() {
            bail!("command root {root} out of range");
        }
        validate(&nodes)?;
        Ok(Self { nodes, root })
    }

    /// Walks `command` (no slash) down the tree as Brigadier parses it.
    pub fn complete(&self, command: &str) -> Completion {
        let words = split_words(command);
        let last = words.len() - 1;
        let mut node = self.root;
        let mut spans = Vec::new();
        let mut i = 0;
        loop {
            let (start, word) = words[i];
            let children = self.children(node);
            let literal = |word: &str| {
                children.iter().copied().find(
                    |&c| matches!(&self.nodes[c].kind, NodeKind::Literal(name) if **name == *word),
                )
            };
            let argument = children.iter().find_map(|&c| match &self.nodes[c].kind {
                NodeKind::Argument(argument) => Some((c, argument)),
                _ => None,
            });
            if i == last {
                let mut literals: Vec<String> = children
                    .iter()
                    .filter_map(|&c| match &self.nodes[c].kind {
                        NodeKind::Literal(name) if name.starts_with(word) => Some(name.to_string()),
                        _ => None,
                    })
                    .collect();
                literals.sort();
                literals.dedup();
                let arguments: Vec<Argument> = children
                    .iter()
                    .filter_map(|&c| match &self.nodes[c].kind {
                        NodeKind::Argument(argument) => Some(argument.clone()),
                        _ => None,
                    })
                    .collect();
                let mut unparsed = false;
                if !word.is_empty() {
                    let kind = if literal(word).is_some() {
                        SpanKind::Literal
                    } else if !arguments.is_empty() {
                        SpanKind::Argument
                    } else {
                        unparsed = true;
                        SpanKind::Unparsed
                    };
                    spans.push(Span {
                        range: start..command.len(),
                        kind,
                    });
                }
                return Completion {
                    start,
                    parent: node,
                    spans,
                    unparsed,
                    literals,
                    arguments,
                };
            }
            if let Some(c) = literal(word) {
                spans.push(Span {
                    range: start..start + word.len(),
                    kind: SpanKind::Literal,
                });
                node = c;
                i += 1;
                continue;
            }
            let Some((c, argument)) = argument else {
                spans.push(Span {
                    range: start..command.len(),
                    kind: SpanKind::Unparsed,
                });
                return Completion {
                    start,
                    parent: node,
                    spans,
                    unparsed: true,
                    ..Completion::default()
                };
            };
            let count = if argument.greedy {
                usize::MAX
            } else {
                argument.words
            };
            if count > last - i {
                spans.push(Span {
                    range: start..command.len(),
                    kind: SpanKind::Argument,
                });
                return Completion {
                    start,
                    parent: node,
                    spans,
                    unparsed: false,
                    literals: Vec::new(),
                    arguments: vec![argument.clone()],
                };
            }
            let (end_start, end_word) = words[i + count - 1];
            spans.push(Span {
                range: start..end_start + end_word.len(),
                kind: SpanKind::Argument,
            });
            node = c;
            i += count;
        }
    }

    /// `CommandNode.getUsageText`.
    fn usage_text(&self, node: usize) -> String {
        match &self.nodes[node].kind {
            NodeKind::Root => String::new(),
            NodeKind::Literal(name) => name.to_string(),
            NodeKind::Argument(argument) => format!("<{}>", argument.name),
        }
    }

    /// Brigadier's private `getSmartUsage(node, source, optional, deep)`.
    fn usage(&self, node: usize, optional: bool, deep: bool) -> String {
        let n = &self.nodes[node];
        let text = self.usage_text(node);
        let this = if optional { format!("[{text}]") } else { text };
        if deep {
            return this;
        }
        if let Some(target) = n.redirect {
            let redirect = if target == self.root {
                "...".to_owned()
            } else {
                format!("-> {}", self.usage_text(target))
            };
            return format!("{this} {redirect}");
        }
        let child_optional = n.executable;
        match n.children.as_slice() {
            [] => this,
            &[child] => {
                let usage = self.usage(child, child_optional, child_optional);
                format!("{this} {usage}")
            }
            children => {
                let mut usages: Vec<String> = Vec::new();
                for &child in children {
                    let usage = self.usage(child, child_optional, true);
                    if !usages.contains(&usage) {
                        usages.push(usage);
                    }
                }
                if let [usage] = usages.as_slice() {
                    let usage = if child_optional {
                        format!("[{usage}]")
                    } else {
                        usage.clone()
                    };
                    return format!("{this} {usage}");
                }
                let (open, close) = if child_optional {
                    ("[", "]")
                } else {
                    ("(", ")")
                };
                let alternatives: Vec<String> =
                    children.iter().map(|&c| self.usage_text(c)).collect();
                format!("{this} {open}{}{close}", alternatives.join("|"))
            }
        }
    }

    /// `CommandSuggestions.fillNodeUsage`.
    pub fn smart_usage(&self, node: usize) -> Vec<String> {
        let optional = self.nodes[node].executable;
        self.children(node)
            .iter()
            .filter(|&&c| matches!(self.nodes[c].kind, NodeKind::Argument(_)))
            .map(|&c| self.usage(c, optional, false))
            .collect()
    }

    /// A node's children, following a redirect (`execute ... run`).
    fn children(&self, node: usize) -> &[usize] {
        let node = &self.nodes[node];
        match node.redirect {
            Some(target) if node.children.is_empty() && target < self.nodes.len() => {
                &self.nodes[target].children
            }
            _ => &node.children,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::protocol::varint::write_var_int;

    fn node(
        buf: &mut Vec<u8>,
        flags: u8,
        children: &[i32],
        name: Option<&str>,
        parser: Option<i32>,
    ) {
        buf.push(flags);
        write_var_int(buf, children.len() as i32).unwrap();
        for &c in children {
            write_var_int(buf, c).unwrap();
        }
        if let Some(name) = name {
            crate::network::protocol::packet::write_string(buf, name).unwrap();
        }
        if let Some(parser) = parser {
            write_var_int(buf, parser).unwrap();
            if parser == 3 {
                buf.push(1);
                buf.extend_from_slice(&0i32.to_be_bytes());
            } else if matches!(parser, 1 | 2 | 4) {
                buf.push(0);
            }
        }
    }

    #[test]
    fn completes_literals_and_knows_arguments() {
        let mut buf = Vec::new();
        write_var_int(&mut buf, 6).unwrap();
        node(&mut buf, 0, &[1, 4], None, None);
        node(&mut buf, 1, &[2, 3], Some("gamemode"), None);
        node(&mut buf, 1 | 4, &[], Some("survival"), None);
        node(&mut buf, 1 | 4, &[], Some("creative"), None);
        node(&mut buf, 1, &[5], Some("give"), None);
        node(&mut buf, 2 | 4, &[], Some("count"), Some(3));
        write_var_int(&mut buf, 0).unwrap();
        let tree = CommandTree::decode(&buf).unwrap();
        let gam = tree.complete("gam");
        assert_eq!((gam.start, gam.literals), (0, vec!["gamemode".to_owned()]));
        let mode = tree.complete("gamemode ");
        assert_eq!(mode.start, 9);
        assert_eq!(mode.literals, ["creative", "survival"]);
        assert!(mode.arguments.is_empty());
        let count = tree.complete("give 1");
        assert_eq!(count.start, 5);
        assert_eq!(count.arguments.len(), 1);
        assert_eq!(count.arguments[0].parser, Parser::Other(3));
        assert_eq!(count.arguments[0].suggestions, None);

        let after_give = tree.complete("give ");
        assert_eq!(tree.smart_usage(after_give.parent), ["<count>"]);
        let spans = tree.complete("give 12 x");
        assert!(spans.unparsed);
        assert_eq!(
            spans
                .spans
                .iter()
                .map(|s| (s.range.clone(), s.kind))
                .collect::<Vec<_>>(),
            [
                (0..4, SpanKind::Literal),
                (5..7, SpanKind::Argument),
                (8..9, SpanKind::Unparsed)
            ]
        );
        assert!(tree.complete("gamem").unparsed);
        assert!(!tree.complete("gamemode").unparsed);
    }

    #[test]
    fn rejects_trees_vanilla_rejects() {
        let tree = |nodes: &[&[i32]]| {
            let mut buf = Vec::new();
            write_var_int(&mut buf, nodes.len() as i32).unwrap();
            node(&mut buf, 0, nodes[0], None, None);
            for children in &nodes[1..] {
                node(&mut buf, 1, children, Some("a"), None);
            }
            write_var_int(&mut buf, 0).unwrap();
            CommandTree::decode(&buf)
        };
        assert!(tree(&[&[1], &[2], &[]]).is_ok());
        assert!(tree(&[&[1], &[3]]).is_err(), "child out of range");
        assert!(tree(&[&[1], &[-1]]).is_err(), "negative child");
        assert!(tree(&[&[1], &[2], &[1]]).is_err(), "cycle");
    }

    #[test]
    fn words_keep_brackets_and_quotes_together() {
        let words = |c| {
            split_words(c)
                .into_iter()
                .map(|(_, w)| w)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            words("tellraw @a {\"text\":\"a b\",\"hover_event\":{\"action\":\"x\"}}"),
            [
                "tellraw",
                "@a",
                "{\"text\":\"a b\",\"hover_event\":{\"action\":\"x\"}}"
            ]
        );
        assert_eq!(
            words("kill @e[type=pig, limit=1] x"),
            ["kill", "@e[type=pig, limit=1]", "x"]
        );
        assert_eq!(
            words("tag @s add \"a b\" c"),
            ["tag", "@s", "add", "\"a b\"", "c"]
        );
        assert_eq!(words("say don't stop"), ["say", "don't", "stop"]);
        assert_eq!(words("give @s stone[a b"), ["give", "@s", "stone[a b"]);
        assert_eq!(words("a "), ["a", ""]);
    }

    #[test]
    fn smart_usage_follows_brigadier() {
        let mut buf = Vec::new();
        write_var_int(&mut buf, 6).unwrap();
        node(&mut buf, 0, &[1, 4], None, None);
        node(&mut buf, 1, &[2], Some("tp"), None);
        node(&mut buf, 2 | 4, &[3], Some("pos"), Some(8));
        node(&mut buf, 2 | 4, &[], Some("yaw"), Some(1));
        node(&mut buf, 1, &[5], Some("say"), None);
        node(&mut buf, 2 | 4, &[], Some("message"), Some(20));
        write_var_int(&mut buf, 0).unwrap();
        let tree = CommandTree::decode(&buf).unwrap();
        assert_eq!(tree.smart_usage(1), ["<pos> [<yaw>]"]);
        let yaw = tree.complete("tp 1 ~ 3 9");
        assert_eq!(yaw.start, 9);
        assert_eq!(yaw.parent, 2);
        assert_eq!(tree.smart_usage(yaw.parent), ["[<yaw>]"]);
        let pos = tree.complete("tp 1 ~");
        assert_eq!((pos.start, pos.parent), (3, 1));
        let message = tree.complete("say hello there");
        assert_eq!(message.start, 4);
        assert!(!message.unparsed);
    }

    #[test]
    fn keeps_what_completing_an_argument_needs() {
        let mut buf = Vec::new();
        write_var_int(&mut buf, 6).unwrap();
        node(&mut buf, 0, &[1, 4], None, None);
        node(&mut buf, 1, &[2], Some("enchant"), None);
        buf.extend([2]);
        write_var_int(&mut buf, 1).unwrap();
        write_var_int(&mut buf, 3).unwrap();
        crate::network::protocol::packet::write_string(&mut buf, "targets").unwrap();
        write_var_int(&mut buf, 6).unwrap();
        buf.push(2);
        buf.push(2 | 4);
        write_var_int(&mut buf, 0).unwrap();
        crate::network::protocol::packet::write_string(&mut buf, "enchantment").unwrap();
        write_var_int(&mut buf, 46).unwrap();
        crate::network::protocol::packet::write_string(&mut buf, "minecraft:enchantment").unwrap();
        node(&mut buf, 1, &[5], Some("tp"), None);
        buf.push(2 | 4 | HAS_SUGGESTION_TYPE);
        write_var_int(&mut buf, 0).unwrap();
        crate::network::protocol::packet::write_string(&mut buf, "target").unwrap();
        write_var_int(&mut buf, 6).unwrap();
        buf.push(1);
        crate::network::protocol::packet::write_string(&mut buf, "minecraft:ask_server").unwrap();
        write_var_int(&mut buf, 0).unwrap();
        let tree = CommandTree::decode(&buf).unwrap();

        let targets = tree.complete("enchant @");
        assert_eq!(targets.start, 8);
        assert_eq!(
            targets.arguments[0].parser,
            Parser::Entity { players_only: true }
        );
        let enchantment = tree.complete("enchant @a minecraft:eff");
        assert_eq!(enchantment.start, 11);
        assert_eq!(
            enchantment.arguments[0].parser,
            Parser::Registry {
                registry: "minecraft:enchantment".into(),
                tags: false
            }
        );
        let target = tree.complete("tp St");
        assert_eq!(
            target.arguments[0].suggestions.as_deref(),
            Some("minecraft:ask_server")
        );
    }
}
