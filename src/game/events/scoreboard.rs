//! The player list and the scoreboard.

use crate::game::{InWorld, PlayerEntry};
use melon_net::play::game::PlayerInfoUpdate;
use melon_net::play::game::scores::ScorePacket;
use melon_world::game_mode::GameMode;

impl InWorld {
    /// `handlePlayerInfoUpdate`: a player is added once (`putIfAbsent`, so
    /// one added again keeps its first profile); updates for players never
    /// added are dropped.
    pub(super) fn update_player_info(&mut self, updates: Vec<PlayerInfoUpdate>) {
        for update in updates {
            if let Some(name) = update.name {
                self.players
                    .entry(update.uuid)
                    .or_insert_with(|| PlayerEntry {
                        name,
                        textures: update.textures,
                        ..PlayerEntry::default()
                    });
            }
            let Some(entry) = self.players.get_mut(&update.uuid) else {
                continue;
            };
            let mut own_mode = None;
            if let Some(mode) = update.game_mode {
                if entry.game_mode != mode && update.uuid == self.uuid {
                    own_mode = Some(mode);
                }
                entry.game_mode = mode;
            }
            if let Some(listed) = update.listed {
                entry.listed = listed;
            }
            if let Some(latency) = update.latency {
                entry.latency = latency;
            }
            if let Some(name) = update.display_name {
                entry.display_name = name;
            }
            if let Some(order) = update.list_order {
                entry.list_order = order;
            }
            if let Some(hat) = update.show_hat {
                entry.show_hat = hat;
            }
            // `LocalPlayer.onGameModeChanged`: a spectator stops rising or falling.
            if own_mode == Some(GameMode::Spectator) {
                let velocity = self.player.velocity();
                self.player.set_velocity(velocity.with_y(0.0));
            }
        }
    }

    /// `handlePlayerInfoRemove`.
    pub(super) fn remove_player_info(&mut self, removed: Vec<uuid::Uuid>) {
        for uuid in removed {
            self.players.remove(&uuid);
        }
    }

    pub(super) fn apply_scores(&mut self, packet: ScorePacket) {
        match packet {
            ScorePacket::TabList { header, footer } => {
                self.tab_list.set_header_footer(header, footer);
            }
            ScorePacket::Scoreboard(update) => self.scoreboard.apply(update),
        }
    }
}
