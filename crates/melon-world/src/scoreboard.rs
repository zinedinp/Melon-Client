//! The client's `Scoreboard`.

use melon_protocol::component::{Component, Contents, Style, TextColor};
use std::collections::HashMap;

/// `ObjectiveCriteria.RenderType`, in wire order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderType {
    #[default]
    Integer,
    Hearts,
}

impl RenderType {
    pub fn from_id(id: i32) -> Self {
        if id == 1 { Self::Hearts } else { Self::Integer }
    }
}

/// `NumberFormat`: how a score's number is shown.
#[derive(Clone, Debug, PartialEq)]
pub enum NumberFormat {
    /// `BlankFormat`: nothing.
    Blank,
    /// `StyledFormat`: the number in this style.
    Styled(Box<Style>),
    /// `FixedFormat`: this text instead of the number.
    Fixed(Box<Component>),
}

impl NumberFormat {
    /// `StyledFormat.PLAYER_LIST_DEFAULT`: yellow.
    pub fn player_list_default() -> Self {
        Self::colored(TextColor::YELLOW)
    }

    /// `StyledFormat.SIDEBAR_DEFAULT`: red.
    pub fn sidebar_default() -> Self {
        Self::colored(TextColor::RED)
    }

    fn colored(color: TextColor) -> Self {
        Self::Styled(Box::new(Style::colored(color)))
    }

    /// `StyledFormat.NO_STYLE`: the number with no colour of its own.
    pub fn no_style() -> Self {
        Self::Styled(Box::default())
    }

    /// `NumberFormat.format`.
    pub fn format(&self, value: i32) -> Component {
        match self {
            Self::Blank => Component::text(""),
            Self::Styled(style) => {
                Component::new(Contents::Text(value.to_string())).with_style((**style).clone())
            }
            Self::Fixed(text) => (**text).clone(),
        }
    }
}

/// `Objective`, as far as the client is told about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Objective {
    pub display_name: Component,
    pub render_type: RenderType,
    /// `numberFormat`: the format of its scores without their own.
    pub number_format: Option<NumberFormat>,
}

/// `Score`: a holder's value in one objective.
#[derive(Clone, Debug, PartialEq)]
pub struct Score {
    pub value: i32,
    /// What the sidebar shows instead of the holder's name.
    pub display: Option<Component>,
    pub number_format: Option<NumberFormat>,
}

impl Score {
    /// `ReadOnlyScoreInfo.formatValue`.
    pub fn format(&self, default: &NumberFormat) -> Component {
        self.number_format
            .as_ref()
            .unwrap_or(default)
            .format(self.value)
    }
}

/// `DisplaySlot`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplaySlot(u8);

impl DisplaySlot {
    pub const LIST: Self = Self(0);
    pub const SIDEBAR: Self = Self(1);
    pub const BELOW_NAME: Self = Self(2);
    const COUNT: usize = 3 + TeamColor::COUNT;

    /// `DisplaySlot.BY_ID`: out of range is `list`.
    pub fn from_id(id: i32) -> Self {
        Self(
            u8::try_from(id)
                .ok()
                .filter(|&id| usize::from(id) < Self::COUNT)
                .unwrap_or(0),
        )
    }

    /// `TeamColor.displaySlot`: the sidebar a team of `color` sees.
    pub fn team_sidebar(color: TeamColor) -> Self {
        Self(3 + color.0)
    }
}

/// `TeamColor`: one of the sixteen chat colours, by id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TeamColor(u8);

impl TeamColor {
    const COUNT: usize = 16;
    /// `TeamColor.BY_ID`: out of range is black.
    pub fn from_id(id: i32) -> Self {
        Self(
            u8::try_from(id)
                .ok()
                .filter(|&id| usize::from(id) < Self::COUNT)
                .unwrap_or(0),
        )
    }

    pub fn named(self) -> TextColor {
        TextColor::NAMED[usize::from(self.0)].1
    }
}

/// `Team.Visibility`: whose name tags show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visibility {
    #[default]
    Always,
    Never,
    HideForOtherTeams,
    HideForOwnTeam,
}

impl Visibility {
    pub fn from_id(id: i32) -> Self {
        match id {
            1 => Self::Never,
            2 => Self::HideForOtherTeams,
            3 => Self::HideForOwnTeam,
            _ => Self::Always,
        }
    }
}

/// `Team.CollisionRule`: who pushes whom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CollisionRule {
    #[default]
    Always,
    Never,
    PushOtherTeams,
    PushOwnTeam,
}

/// `EntitySelector.pushableBy`'s team part.
pub fn team_allows_push(
    own: Option<(&str, CollisionRule)>,
    their: Option<(&str, CollisionRule)>,
) -> bool {
    let own_rule = own.map_or(CollisionRule::Always, |t| t.1);
    let their_rule = their.map_or(CollisionRule::Always, |t| t.1);
    if own_rule == CollisionRule::Never || their_rule == CollisionRule::Never {
        return false;
    }
    let same_team = matches!((own, their), (Some(a), Some(b)) if a.0 == b.0);
    if (own_rule == CollisionRule::PushOwnTeam || their_rule == CollisionRule::PushOwnTeam)
        && same_team
    {
        return false;
    }
    (own_rule != CollisionRule::PushOtherTeams && their_rule != CollisionRule::PushOtherTeams)
        || same_team
}

impl CollisionRule {
    pub fn from_id(id: i32) -> Self {
        match id {
            1 => Self::Never,
            2 => Self::PushOtherTeams,
            3 => Self::PushOwnTeam,
            _ => Self::Always,
        }
    }
}

/// `ClientboundSetPlayerTeamPacket.Parameters`: what a team looks like.
#[derive(Clone, Debug, PartialEq)]
pub struct TeamLook {
    pub display_name: Component,
    pub prefix: Component,
    pub suffix: Component,
    pub name_tags: Visibility,
    pub collision: CollisionRule,
    pub color: Option<TeamColor>,
    /// `PlayerTeam.packOptions`.
    pub options: u8,
}

impl Default for TeamLook {
    fn default() -> Self {
        Self {
            display_name: Component::text(""),
            prefix: Component::text(""),
            suffix: Component::text(""),
            name_tags: Visibility::Always,
            collision: CollisionRule::Always,
            color: None,
            options: 0,
        }
    }
}

impl TeamLook {
    pub fn friendly_fire(&self) -> bool {
        self.options & 1 != 0
    }

    pub fn sees_friendly_invisibles(&self) -> bool {
        self.options & 2 != 0
    }

    /// `PlayerTeam.getFormattedName`.
    pub fn format_name(&self, name: Component) -> Component {
        let mut out = Component::text("");
        if let Some(color) = self.color {
            out.style.color = Some(color.named());
        }
        out.extra = vec![self.prefix.clone(), name, self.suffix.clone()];
        out
    }
}

/// `PlayerTeam`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Team {
    pub look: TeamLook,
    pub members: Vec<Box<str>>,
}

/// What the server changes, one packet each.
#[derive(Clone, Debug, PartialEq)]
pub enum ScoreUpdate {
    /// `ClientboundSetObjectivePacket` add (0) and change (2).
    SetObjective {
        name: Box<str>,
        objective: Objective,
        add: bool,
    },
    /// `ClientboundSetObjectivePacket` remove (1).
    RemoveObjective(Box<str>),
    /// `ClientboundSetDisplayObjectivePacket`; `None` clears the slot.
    Display {
        slot: DisplaySlot,
        objective: Option<Box<str>>,
    },
    /// `ClientboundSetScorePacket`.
    SetScore {
        holder: Box<str>,
        objective: Box<str>,
        score: Score,
    },
    /// `ClientboundResetScorePacket`; `None` resets every objective.
    ResetScore {
        holder: Box<str>,
        objective: Option<Box<str>>,
    },
    /// `ClientboundSetPlayerTeamPacket`.
    Team { name: Box<str>, action: TeamAction },
}

/// `ClientboundSetPlayerTeamPacket`'s methods.
#[derive(Clone, Debug, PartialEq)]
pub enum TeamAction {
    Add {
        look: Box<TeamLook>,
        players: Vec<Box<str>>,
    },
    Remove,
    Change(Box<TeamLook>),
    Join(Vec<Box<str>>),
    Leave(Vec<Box<str>>),
}

/// The sidebar's most rows (`Hud.displayScoreboardSidebar`'s `limit(15)`).
pub const SIDEBAR_ROWS: usize = 15;

#[derive(Debug, Default)]
pub struct Scoreboard {
    objectives: HashMap<Box<str>, Objective>,
    /// By holder, then objective.
    scores: HashMap<Box<str>, HashMap<Box<str>, Score>>,
    /// The objective each [`DisplaySlot`] shows.
    display: [Option<Box<str>>; DisplaySlot::COUNT],
    teams: HashMap<Box<str>, Team>,
    /// Holder to team name.
    team_of: HashMap<Box<str>, Box<str>>,
}

impl Scoreboard {
    /// `ClientPacketListener.handleAddObjective`, `handleSetScore` and the other scoreboard packets.
    pub fn apply(&mut self, update: ScoreUpdate) {
        match update {
            ScoreUpdate::SetObjective {
                name,
                objective,
                add,
            } => {
                if add {
                    self.objectives.entry(name).or_insert(objective);
                } else if let Some(old) = self.objectives.get_mut(&name) {
                    *old = objective;
                }
            }
            ScoreUpdate::RemoveObjective(name) => self.remove_objective(&name),
            ScoreUpdate::Display { slot, objective } => {
                self.display[usize::from(slot.0)] =
                    objective.filter(|name| self.objectives.contains_key(name));
            }
            ScoreUpdate::SetScore {
                holder,
                objective,
                score,
            } => {
                if self.objectives.contains_key(&objective) {
                    self.scores
                        .entry(holder)
                        .or_default()
                        .insert(objective, score);
                }
            }
            ScoreUpdate::ResetScore { holder, objective } => match objective {
                None => {
                    self.scores.remove(&holder);
                }
                Some(objective) => {
                    if let Some(scores) = self.scores.get_mut(&holder) {
                        scores.remove(&objective);
                        if scores.is_empty() {
                            self.scores.remove(&holder);
                        }
                    }
                }
            },
            ScoreUpdate::Team { name, action } => self.apply_team(name, action),
        }
    }

    fn apply_team(&mut self, name: Box<str>, action: TeamAction) {
        match action {
            TeamAction::Add { look, players } => {
                if !self.teams.contains_key(&name) {
                    self.teams.insert(
                        name.clone(),
                        Team {
                            look: *look,
                            members: Vec::new(),
                        },
                    );
                }
                self.join(&name, players);
            }
            TeamAction::Remove => {
                if let Some(team) = self.teams.remove(&name) {
                    for member in team.members {
                        self.team_of.remove(&member);
                    }
                }
            }
            TeamAction::Change(look) => {
                if let Some(team) = self.teams.get_mut(&name) {
                    team.look = *look;
                }
            }
            TeamAction::Join(players) => self.join(&name, players),
            TeamAction::Leave(players) => {
                for player in players {
                    if self.team_of.get(&player) == Some(&name) {
                        self.team_of.remove(&player);
                        if let Some(team) = self.teams.get_mut(&name) {
                            team.members.retain(|m| *m != player);
                        }
                    }
                }
            }
        }
    }

    /// `addPlayerToTeam`: out of its old team first.
    fn join(&mut self, name: &str, players: Vec<Box<str>>) {
        if !self.teams.contains_key(name) {
            return;
        }
        for player in players {
            if let Some(old) = self.team_of.remove(&player)
                && let Some(team) = self.teams.get_mut(&old)
            {
                team.members.retain(|m| *m != player);
            }
            if let Some(team) = self.teams.get_mut(name) {
                team.members.push(player.clone());
            }
            self.team_of.insert(player, name.into());
        }
    }

    /// `removeObjective`: its scores go, and the slots showing it clear.
    fn remove_objective(&mut self, name: &str) {
        if self.objectives.remove(name).is_none() {
            return;
        }
        for slot in &mut self.display {
            if slot.as_deref() == Some(name) {
                *slot = None;
            }
        }
        self.scores.retain(|_, scores| {
            scores.remove(name);
            !scores.is_empty()
        });
    }

    /// `getDisplayObjective`: the objective `slot` shows, by name.
    pub fn display_objective(&self, slot: DisplaySlot) -> Option<(&str, &Objective)> {
        let name = self.display[usize::from(slot.0)].as_deref()?;
        Some((name, self.objectives.get(name)?))
    }

    /// `getPlayerScoreInfo`.
    pub fn score(&self, holder: &str, objective: &str) -> Option<&Score> {
        self.scores.get(holder)?.get(objective)
    }

    /// `getPlayersTeam`: the team `holder` is in, by name.
    pub fn team_of(&self, holder: &str) -> Option<(&str, &Team)> {
        let name = self.team_of.get(holder)?;
        Some((name, self.teams.get(name)?))
    }

    /// `PlayerTeam.formatNameForTeam`: `name` as `holder`'s team shows it.
    pub fn format_name(&self, holder: &str, name: Component) -> Component {
        match self.team_of(holder) {
            Some((_, team)) => team.look.format_name(name),
            None => name,
        }
    }

    /// `Entity.belowNameDisplay`.
    pub fn below_name_display(&self, holder: &str) -> Option<Component> {
        let (name, objective) = self.display_objective(DisplaySlot::BELOW_NAME)?;
        let score = self.score(holder, name)?;
        let formatted = score.format(
            objective
                .number_format
                .as_ref()
                .unwrap_or(&NumberFormat::no_style()),
        );
        Some(
            Component::empty()
                .append(formatted)
                .append(Component::text(" "))
                .append(objective.display_name.clone()),
        )
    }

    /// `Hud.extractScoreboardSidebar`'s choice.
    pub fn sidebar_objective(&self, own_name: &str) -> Option<(&str, &Objective)> {
        self.team_of(own_name)
            .and_then(|(_, team)| team.look.color)
            .and_then(|color| self.display_objective(DisplaySlot::team_sidebar(color)))
            .or_else(|| self.display_objective(DisplaySlot::SIDEBAR))
    }

    /// `Hud.displayScoreboardSidebar`'s rows.
    pub fn sidebar_entries(&self, objective: &str) -> Vec<(&str, &Score)> {
        let mut out: Vec<(&str, &Score)> = self
            .scores
            .iter()
            .filter(|(holder, _)| !holder.starts_with('#'))
            .filter_map(|(holder, scores)| Some((&**holder, scores.get(objective)?)))
            .collect();
        out.sort_by(|(a, x), (b, y)| {
            y.value
                .cmp(&x.value)
                .then_with(|| compare_ignore_case(a, b))
        });
        out.truncate(SIDEBAR_ROWS);
        out
    }

    /// Every team, by name: the spectator menu's "Teleport to Team".
    pub fn teams(&self) -> impl Iterator<Item = (&str, &Team)> {
        self.teams.iter().map(|(name, team)| (&**name, team))
    }
}

/// `String.compareToIgnoreCase`: by each char upper- then lower-cased.
pub fn compare_ignore_case(a: &str, b: &str) -> std::cmp::Ordering {
    let fold = |c: char| {
        let upper = c.to_uppercase().next().unwrap_or(c);
        upper.to_lowercase().next().unwrap_or(upper)
    };
    a.chars().map(fold).cmp(b.chars().map(fold))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_compare_without_case() {
        use std::cmp::Ordering;
        assert_eq!(compare_ignore_case("alex", "Alex"), Ordering::Equal);
        assert_eq!(compare_ignore_case("Bob", "alex"), Ordering::Greater);
        assert_eq!(compare_ignore_case("al", "Alex"), Ordering::Less);
    }

    #[test]
    fn the_sidebar_lists_visible_scores_highest_first() {
        let mut board = Scoreboard::default();
        board.apply(ScoreUpdate::SetObjective {
            name: "kills".into(),
            objective: objective(RenderType::Integer),
            add: true,
        });
        for (holder, value) in [("bob", 3), ("Alex", 3), ("#hidden", 9), ("zed", 7)] {
            board.apply(ScoreUpdate::SetScore {
                holder: holder.into(),
                objective: "kills".into(),
                score: score(value),
            });
        }
        let rows: Vec<&str> = board
            .sidebar_entries("kills")
            .into_iter()
            .map(|(holder, _)| holder)
            .collect();
        assert_eq!(rows, ["zed", "Alex", "bob"]);
    }

    fn objective(render_type: RenderType) -> Objective {
        Objective {
            display_name: Component::text("Kills"),
            render_type,
            number_format: None,
        }
    }

    fn score(value: i32) -> Score {
        Score {
            value,
            display: None,
            number_format: None,
        }
    }

    #[test]
    fn scores_need_their_objective_and_go_with_it() {
        let mut board = Scoreboard::default();
        board.apply(ScoreUpdate::SetScore {
            holder: "Steve".into(),
            objective: "kills".into(),
            score: score(3),
        });
        assert!(board.score("Steve", "kills").is_none());
        board.apply(ScoreUpdate::SetObjective {
            name: "kills".into(),
            objective: objective(RenderType::Integer),
            add: true,
        });
        board.apply(ScoreUpdate::SetScore {
            holder: "Steve".into(),
            objective: "kills".into(),
            score: score(3),
        });
        board.apply(ScoreUpdate::Display {
            slot: DisplaySlot::LIST,
            objective: Some("kills".into()),
        });
        assert_eq!(board.score("Steve", "kills").map(|s| s.value), Some(3));
        assert_eq!(
            board.display_objective(DisplaySlot::LIST).map(|o| o.0),
            Some("kills")
        );
        board.apply(ScoreUpdate::RemoveObjective("kills".into()));
        assert!(board.score("Steve", "kills").is_none());
        assert!(board.display_objective(DisplaySlot::LIST).is_none());
    }

    #[test]
    fn a_player_is_in_one_team_at_a_time() {
        let mut board = Scoreboard::default();
        for name in ["red", "blue"] {
            board.apply(ScoreUpdate::Team {
                name: name.into(),
                action: TeamAction::Add {
                    look: Box::default(),
                    players: Vec::new(),
                },
            });
        }
        let join = |team: &str| ScoreUpdate::Team {
            name: team.into(),
            action: TeamAction::Join(vec!["Alex".into()]),
        };
        board.apply(join("red"));
        board.apply(join("blue"));
        assert_eq!(board.team_of("Alex").map(|t| t.0), Some("blue"));
        assert!(
            board
                .teams()
                .all(|(name, team)| (name == "blue") == !team.members.is_empty())
        );
        board.apply(ScoreUpdate::Team {
            name: "red".into(),
            action: TeamAction::Leave(vec!["Alex".into()]),
        });
        assert_eq!(board.team_of("Alex").map(|t| t.0), Some("blue"));
        board.apply(ScoreUpdate::Team {
            name: "blue".into(),
            action: TeamAction::Remove,
        });
        assert!(board.team_of("Alex").is_none());
    }

    #[test]
    fn team_names_wrap_prefix_name_and_suffix_in_the_colour() {
        let look = TeamLook {
            prefix: Component::text("["),
            suffix: Component::text("]"),
            color: Some(TeamColor::from_id(12)),
            ..TeamLook::default()
        };
        let name = look.format_name(Component::text("Alex"));
        assert_eq!(name.style.color, Some(TextColor::RED));
        assert_eq!(name.extra.len(), 3);
        assert_eq!(name.plain_text(), "[Alex]");
    }

    #[test]
    fn a_team_name_keeps_everything_a_prefix_and_suffix_can_carry() {
        use melon_assets::i18n::Lang;
        use melon_assets::text::StyledText;
        use serde_json::json;
        let prefix = melon_protocol::component::decode::component(&json!([
            {"text": "A", "color": "#12ab34", "bold": true, "font": "minecraft:alt"},
            {"translate": "rank.%s", "with": [{"text": "x", "obfuscated": true}], "italic": true},
            {"object": "atlas", "sprite": "minecraft:item/apple"},
            "\u{a7}cR\u{a7}lB\u{a7}rN "
        ]));
        let look = TeamLook {
            prefix,
            suffix: Component::text(" \u{a7}7!"),
            color: Some(TeamColor::from_id(12)),
            ..TeamLook::default()
        };
        let text = StyledText::from_component(
            &look.format_name(Component::text("Alex")),
            &Lang::from_pairs(&[("rank.%s", "[%s]")]),
        );
        let span = |needle: &str| {
            text.spans
                .iter()
                .find(|s| s.text.contains(needle))
                .unwrap_or_else(|| panic!("no span with {needle}: {text:?}"))
        };
        let red = TextColor::RED.rgb();
        let a = span("A");
        assert_eq!(a.style.color, Some(TextColor([0x12, 0xab, 0x34]).rgb()));
        assert!(a.style.bold);
        assert_eq!(a.style.font.as_deref(), Some("minecraft:alt"));
        assert!(span("[").style.italic && span("[").style.bold);
        assert!(span("x").style.obfuscated);
        assert!(text.spans.iter().any(|s| s.style.sprite.is_some()));
        assert!(span("B").style.bold && span("B").style.color == Some(red));
        assert_eq!(
            span("N").style.color,
            Some(TextColor([0x12, 0xab, 0x34]).rgb())
        );
        assert_eq!(span("Alex").style.color, Some(red));
        assert!(!span("Alex").style.bold);
        assert_ne!(span("!").style.color, Some(red));
    }

    #[test]
    fn formats_fall_back_from_score_to_default() {
        let default = NumberFormat::player_list_default();
        let mut s = score(7);
        let shown = s.format(&default);
        assert_eq!(shown.style.color, Some(TextColor::YELLOW));
        s.number_format = Some(NumberFormat::Fixed(Box::new(Component::text("-"))));
        assert_eq!(s.format(&default).plain_text(), "-");
        assert_eq!(NumberFormat::Blank.format(7).plain_text(), "");
    }

    #[test]
    fn below_name_is_the_score_a_space_and_the_objective_title() {
        let mut board = Scoreboard::default();
        board.apply(ScoreUpdate::SetObjective {
            name: "money".into(),
            objective: Objective {
                display_name: Component::text("Coins"),
                render_type: RenderType::Integer,
                number_format: None,
            },
            add: true,
        });
        board.apply(ScoreUpdate::SetScore {
            holder: "Alex".into(),
            objective: "money".into(),
            score: score(1500),
        });
        board.apply(ScoreUpdate::Display {
            slot: DisplaySlot::BELOW_NAME,
            objective: Some("money".into()),
        });
        assert_eq!(
            board.below_name_display("Alex").unwrap().plain_text(),
            "1500 Coins"
        );
        assert!(board.below_name_display("Steve").is_none());
    }

    #[test]
    fn the_own_team_colour_picks_its_sidebar() {
        let mut board = Scoreboard::default();
        for name in ["all", "reds"] {
            board.apply(ScoreUpdate::SetObjective {
                name: name.into(),
                objective: objective(RenderType::Integer),
                add: true,
            });
        }
        board.apply(ScoreUpdate::Display {
            slot: DisplaySlot::SIDEBAR,
            objective: Some("all".into()),
        });
        board.apply(ScoreUpdate::Display {
            slot: DisplaySlot::from_id(15),
            objective: Some("reds".into()),
        });
        assert_eq!(board.sidebar_objective("Steve").map(|o| o.0), Some("all"));
        board.apply(ScoreUpdate::Team {
            name: "r".into(),
            action: TeamAction::Add {
                look: Box::new(TeamLook {
                    color: Some(TeamColor::from_id(12)),
                    ..TeamLook::default()
                }),
                players: vec!["Steve".into()],
            },
        });
        assert_eq!(board.sidebar_objective("Steve").map(|o| o.0), Some("reds"));
    }
}

#[cfg(test)]
mod push_rule_tests {
    use super::*;
    use CollisionRule::*;

    fn team(name: &str, rule: CollisionRule) -> Option<(&str, CollisionRule)> {
        Some((name, rule))
    }

    #[test]
    fn without_teams_everything_pushes() {
        assert!(team_allows_push(None, None));
    }

    #[test]
    fn never_stops_the_push_from_either_side() {
        assert!(!team_allows_push(team("a", Never), None));
        assert!(!team_allows_push(None, team("b", Never)));
    }

    #[test]
    fn push_own_team_spares_team_mates_only() {
        assert!(!team_allows_push(team("a", PushOwnTeam), team("a", Always)));
        assert!(team_allows_push(team("a", PushOwnTeam), team("b", Always)));
    }

    #[test]
    fn push_other_teams_pushes_only_across_teams() {
        assert!(team_allows_push(
            team("a", PushOtherTeams),
            team("a", PushOtherTeams)
        ));
        assert!(!team_allows_push(team("a", PushOtherTeams), None));
        assert!(!team_allows_push(
            team("a", PushOtherTeams),
            team("b", Always)
        ));
    }
}
