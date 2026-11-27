//! The player list as the tab list, the spectator menu and chat completion see it.

use super::InWorld;
use super::PlayerEntry;
use super::world_resources::WorldResources;
use melon_protocol::component::Component;
use melon_gui::hud::item_slot::ItemPainter;
use melon_gui::hud::player_tab::{MAX_PLAYERS, TabDraw, TabEntry};
use melon_gui::hud::sidebar::{self, SidebarRow};
use melon_gui::text::StyledText;
use melon_assets::i18n::Lang;
use melon_render::quad::DrawList;
use melon_world::entity::magic_name::Magic;
use melon_world::scoreboard::{DisplaySlot, NumberFormat, compare_ignore_case};

impl InWorld {
    /// `TeleportToPlayerMenuCategory`.
    pub fn teleport_targets(&self) -> Vec<(uuid::Uuid, String)> {
        let mut out: Vec<_> = self
            .players
            .iter()
            .filter(|(_, p)| p.listed && !p.game_mode.is_spectator())
            .map(|(id, p)| (*id, p.name.clone()))
            .collect();
        out.sort_by_key(|(id, _)| *id);
        out
    }

    /// Picks from the spectator menu, teleporting when a player is chosen.
    pub fn spectator_action(
        &mut self,
        action: melon_gui::overlay::spectator_menu::SpectatorAction,
    ) {
        if let melon_gui::overlay::spectator_menu::SpectatorAction::TeleportTo(id) = action {
            self.joined.session.send_teleport_to_entity(id);
        }
    }

    /// Player names for chat completion, the own one included.
    pub fn player_names(&self) -> Vec<String> {
        self.players.values().map(|p| p.name.clone()).collect()
    }

    /// The skin the server named for `id` (the player list's `textures`).
    pub fn profile_textures(&self, id: uuid::Uuid) -> Option<&str> {
        self.players.get(&id)?.textures.as_deref()
    }

    /// Whether players' faces are drawn now.
    pub(super) fn faces_shown(&self) -> bool {
        self.tab_list.visible() || self.game_mode.current.is_spectator()
    }

    /// `getListedOnlinePlayers` sorted by `PlayerTabOverlay.PLAYER_COMPARATOR`.
    pub(super) fn listed_players(&self) -> Vec<(uuid::Uuid, &PlayerEntry)> {
        let mut out: Vec<_> = self
            .players
            .iter()
            .filter(|(_, p)| p.listed)
            .map(|(id, p)| (*id, p))
            .collect();
        let team = |p: &PlayerEntry| self.scoreboard.team_of(&p.name).map_or("", |t| t.0);
        out.sort_by(|(_, a), (_, b)| {
            b.list_order
                .cmp(&a.list_order)
                .then_with(|| a.game_mode.is_spectator().cmp(&b.game_mode.is_spectator()))
                .then_with(|| team(a).cmp(team(b)))
                .then_with(|| compare_ignore_case(&a.name, &b.name))
        });
        out.truncate(MAX_PLAYERS);
        out
    }

    /// `Hud.extractTabList`.
    fn update_tab_visibility(&mut self) -> bool {
        let objective = self
            .scoreboard
            .display_objective(DisplaySlot::LIST)
            .is_some();
        let alone = self.players.values().filter(|p| p.listed).count() <= 1;
        let shown = self.keys.player_list
            && self.overlay.is_none()
            && !(self.is_integrated() && alone && !objective);
        self.tab_list.set_visible(shown);
        shown
    }

    /// The tab list, while it is shown.
    pub(super) fn draw_tab_list(
        &mut self,
        draw_list: &mut DrawList,
        assets: &WorldResources,
        painter: ItemPainter,
        lang: &Lang,
        width: f32,
    ) {
        if !self.update_tab_visibility() {
            return;
        }
        let mut overlay = std::mem::take(&mut self.tab_list);
        let entries = self.tab_entries(assets, lang);
        overlay.draw(
            draw_list,
            &TabDraw {
                textures: &assets.tab,
                hearts: assets.hud.hearts(),
                painter,
                lang,
                entries: &entries,
                objective: self
                    .scoreboard
                    .display_objective(DisplaySlot::LIST)
                    .map(|o| o.1),
                show_heads: self.online_mode || self.is_integrated(),
                width,
                ticks: self.hud.ticks(),
            },
        );
        drop(entries);
        self.tab_list = overlay;
    }

    /// `Hud.extractScoreboardSidebar`.
    pub(super) fn draw_sidebar(
        &self,
        draw_list: &mut DrawList,
        painter: ItemPainter,
        lang: &Lang,
        size: (f32, f32),
    ) {
        let own = self.players.get(&self.uuid).map_or("", |p| p.name.as_str());
        let Some((name, objective)) = self.scoreboard.sidebar_objective(own) else {
            return;
        };
        let default = objective
            .number_format
            .clone()
            .unwrap_or_else(NumberFormat::sidebar_default);
        let rows: Vec<SidebarRow> = self
            .scoreboard
            .sidebar_entries(name)
            .into_iter()
            .map(|(holder, score)| {
                let shown = score
                    .display
                    .clone()
                    .unwrap_or_else(|| Component::text(holder.to_owned()));
                SidebarRow {
                    name: StyledText::from_component(
                        &self.scoreboard.format_name(holder, shown),
                        lang,
                    ),
                    score: StyledText::from_component(&score.format(&default), lang),
                }
            })
            .collect();
        let title = StyledText::from_component(&objective.display_name, lang);
        sidebar::draw(draw_list, painter, &title, &rows, size);
    }

    /// Each listed player as the tab list shows them.
    fn tab_entries(&self, assets: &WorldResources, lang: &Lang) -> Vec<TabEntry<'_>> {
        let objective = self
            .scoreboard
            .display_objective(DisplaySlot::LIST)
            .map(|o| o.0);
        self.listed_players()
            .into_iter()
            .map(|(id, p)| {
                let spectator = p.game_mode.is_spectator();
                let mut name = match &p.display_name {
                    Some(name) => name.clone(),
                    None => self
                        .scoreboard
                        .format_name(&p.name, Component::text(p.name.clone())),
                };
                if spectator {
                    name.style.italic = Some(true);
                }
                let flip = Magic::of(&p.name) == Magic::UpsideDown
                    && (id == self.uuid || self.entities.iter().any(|e| e.uuid == id));
                if p.display_name.is_some() {
                    melon_core::probe::note("tab.text", &p.name, || tab_text_detail(&name, lang));
                }
                TabEntry {
                    id,
                    name: StyledText::from_component(&name, lang),
                    spectator,
                    latency: p.latency,
                    hat: p.show_hat,
                    flip,
                    skin: assets.players.for_profile(id).texture,
                    score: objective.and_then(|o| self.scoreboard.score(&p.name, o)),
                }
            })
            .collect()
    }
}

/// The text of a tab entry and the fonts its runs name, for the probe.
fn tab_text_detail(name: &Component, lang: &Lang) -> String {
    let styled = StyledText::from_component(name, lang);
    let fonts: Vec<&str> = styled
        .spans
        .iter()
        .filter_map(|s| s.style.font.as_deref())
        .collect();
    format!("{:?} fonts={fonts:?} raw={name:?}", styled.to_plain())
}
