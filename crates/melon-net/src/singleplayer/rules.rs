//! A world's game rules, as the integrated server keeps them.

use std::collections::BTreeMap;
use std::path::Path;

use pumpkin_data::game_rules::{GameRule, GameRuleRegistry, GameRuleValue};
use pumpkin_world::world_info::MAXIMUM_SUPPORTED_WORLD_DATA_VERSION;
use pumpkin_world::world_info::data_files::{read_game_rules, write_game_rules};

/// A rule's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Int(i64),
}

/// The rules that differ from their default, by name.
pub type Changes = BTreeMap<String, Value>;

/// `GameRuleCategory`, in the order of its ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Chat,
    Drops,
    Misc,
    Mobs,
    Player,
    Spawning,
    Updates,
}

impl Category {
    pub fn id(self) -> &'static str {
        match self {
            Category::Chat => "chat",
            Category::Drops => "drops",
            Category::Misc => "misc",
            Category::Mobs => "mobs",
            Category::Player => "player",
            Category::Spawning => "spawning",
            Category::Updates => "updates",
        }
    }

    /// `GameRuleCategory.label`: `gamerule.category.<id>`.
    pub fn lang_key(self) -> String {
        format!("gamerule.category.{}", self.id())
    }
}

/// One rule.
pub struct Rule {
    pub name: String,
    pub default: Value,
    pub category: Category,
    /// An integer rule's range.
    pub min: i64,
    pub max: i64,
    /// The feature pack that has to be on for the rule to show.
    pub feature: Option<&'static str>,
}

impl Rule {
    /// `GameRule.serialize`: the text of a value.
    pub fn text(value: Value) -> String {
        match value {
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
        }
    }

    /// `GameRule.deserialize` for an integer rule: a number in range.
    pub fn parse_int(&self, text: &str) -> Option<i64> {
        let n: i64 = text.trim().parse().ok()?;
        (self.min..=self.max).contains(&n).then_some(n)
    }
}

/// `GameRules`' registrations.
const META: &[(&str, Category, i64, i64, Option<&str>)] = &[
    ("advance_time", Category::Updates, 0, 0, None),
    ("advance_weather", Category::Updates, 0, 0, None),
    (
        "allow_entering_nether_using_portals",
        Category::Misc,
        0,
        0,
        None,
    ),
    ("block_drops", Category::Drops, 0, 0, None),
    ("block_explosion_drop_decay", Category::Drops, 0, 0, None),
    ("command_blocks_work", Category::Misc, 0, 0, None),
    ("command_block_output", Category::Chat, 0, 0, None),
    ("drowning_damage", Category::Player, 0, 0, None),
    ("elytra_movement_check", Category::Player, 0, 0, None),
    ("ender_pearls_vanish_on_death", Category::Player, 0, 0, None),
    ("entity_drops", Category::Drops, 0, 0, None),
    ("fall_damage", Category::Player, 0, 0, None),
    ("fire_damage", Category::Player, 0, 0, None),
    (
        "fire_spread_radius_around_player",
        Category::Updates,
        -1,
        i32::MAX as i64,
        None,
    ),
    ("forgive_dead_players", Category::Mobs, 0, 0, None),
    ("freeze_damage", Category::Player, 0, 0, None),
    ("global_sound_events", Category::Misc, 0, 0, None),
    ("immediate_respawn", Category::Player, 0, 0, None),
    ("keep_inventory", Category::Player, 0, 0, None),
    ("lava_source_conversion", Category::Updates, 0, 0, None),
    ("limited_crafting", Category::Player, 0, 0, None),
    ("locator_bar", Category::Player, 0, 0, None),
    ("log_admin_commands", Category::Chat, 0, 0, None),
    (
        "max_block_modifications",
        Category::Misc,
        1,
        i32::MAX as i64,
        None,
    ),
    (
        "max_command_forks",
        Category::Misc,
        0,
        i32::MAX as i64,
        None,
    ),
    (
        "max_command_sequence_length",
        Category::Misc,
        0,
        i32::MAX as i64,
        None,
    ),
    (
        "max_entity_cramming",
        Category::Mobs,
        0,
        i32::MAX as i64,
        None,
    ),
    (
        "max_minecart_speed",
        Category::Misc,
        1,
        1000,
        Some("minecart_improvements"),
    ),
    (
        "max_snow_accumulation_height",
        Category::Updates,
        0,
        8,
        None,
    ),
    ("mob_drops", Category::Drops, 0, 0, None),
    ("mob_explosion_drop_decay", Category::Drops, 0, 0, None),
    ("mob_griefing", Category::Mobs, 0, 0, None),
    ("natural_health_regeneration", Category::Player, 0, 0, None),
    ("player_movement_check", Category::Player, 0, 0, None),
    (
        "players_nether_portal_creative_delay",
        Category::Player,
        0,
        i32::MAX as i64,
        None,
    ),
    (
        "players_nether_portal_default_delay",
        Category::Player,
        0,
        i32::MAX as i64,
        None,
    ),
    (
        "players_sleeping_percentage",
        Category::Player,
        0,
        i32::MAX as i64,
        None,
    ),
    ("projectiles_can_break_blocks", Category::Drops, 0, 0, None),
    ("pvp", Category::Player, 0, 0, None),
    ("raids", Category::Mobs, 0, 0, None),
    (
        "random_tick_speed",
        Category::Updates,
        0,
        i32::MAX as i64,
        None,
    ),
    ("reduced_debug_info", Category::Misc, 0, 0, None),
    ("respawn_radius", Category::Player, 0, i32::MAX as i64, None),
    ("send_command_feedback", Category::Chat, 0, 0, None),
    ("show_advancement_messages", Category::Chat, 0, 0, None),
    ("show_death_messages", Category::Chat, 0, 0, None),
    ("spawner_blocks_work", Category::Misc, 0, 0, None),
    ("spawn_mobs", Category::Spawning, 0, 0, None),
    ("spawn_monsters", Category::Spawning, 0, 0, None),
    ("spawn_patrols", Category::Spawning, 0, 0, None),
    ("spawn_phantoms", Category::Spawning, 0, 0, None),
    ("spawn_wandering_traders", Category::Spawning, 0, 0, None),
    ("spawn_wardens", Category::Spawning, 0, 0, None),
    ("spectators_generate_chunks", Category::Player, 0, 0, None),
    ("spread_vines", Category::Updates, 0, 0, None),
    ("tnt_explodes", Category::Misc, 0, 0, None),
    ("tnt_explosion_drop_decay", Category::Drops, 0, 0, None),
    ("universal_anger", Category::Mobs, 0, 0, None),
    ("water_source_conversion", Category::Updates, 0, 0, None),
];

fn value_of(registry: &GameRuleRegistry, rule: &GameRule) -> Value {
    match registry.get(rule) {
        GameRuleValue::Bool(b) => Value::Bool(*b),
        GameRuleValue::Int(i) => Value::Int(*i),
    }
}

/// Every rule, by display name.
pub fn all() -> Vec<Rule> {
    let defaults = GameRuleRegistry::default();
    let mut rules: Vec<Rule> = GameRule::all()
        .iter()
        .map(|rule| {
            let name = rule.to_string();
            let meta = META.iter().find(|m| m.0 == name);
            Rule {
                default: value_of(&defaults, rule),
                category: meta.map_or(Category::Misc, |m| m.1),
                min: meta.map_or(i64::from(i32::MIN), |m| m.2),
                max: meta.map_or(i64::from(i32::MAX), |m| m.3),
                feature: meta.and_then(|m| m.4),
                name,
            }
        })
        .collect();
    rules.sort_by(|a, b| a.name.cmp(&b.name));
    rules
}

/// The language key of a rule's caption, `gamerule.<camelCase>`.
pub fn lang_key(name: &str) -> String {
    let mut key = String::from("gamerule.");
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            key.extend(c.to_uppercase());
            upper = false;
        } else {
            key.push(c);
        }
    }
    key
}

/// `max_entity_cramming` as "Max Entity Cramming".
pub fn humanized(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The rules a world's file has that differ from the defaults.
pub fn read_changes(world_dir: &Path) -> Changes {
    let defaults = GameRuleRegistry::default();
    let stored = read_game_rules(world_dir);
    GameRule::all()
        .iter()
        .filter_map(|rule| {
            let value = value_of(&stored, rule);
            (value != value_of(&defaults, rule)).then(|| (rule.to_string(), value))
        })
        .collect()
}

/// Writes the world's rules: the defaults with `changes`.
pub fn write_changes(world_dir: &Path, changes: &Changes) -> std::io::Result<()> {
    let mut registry = GameRuleRegistry::default();
    for rule in GameRule::all() {
        let Some(&value) = changes.get(&rule.to_string()) else {
            continue;
        };
        match (registry.get_mut(rule), value) {
            (GameRuleValue::Bool(slot), Value::Bool(b)) => *slot = b,
            (GameRuleValue::Int(slot), Value::Int(i)) => *slot = i,
            _ => {}
        }
    }
    write_game_rules(world_dir, &registry, MAXIMUM_SUPPORTED_WORLD_DATA_VERSION)
        .map_err(|e| std::io::Error::other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_rules_know_their_range() {
        let rules = all();
        let rule = |name: &str| rules.iter().find(|r| r.name == name).unwrap();
        let snow = rule("max_snow_accumulation_height");
        assert_eq!(snow.parse_int("8"), Some(8));
        assert_eq!(snow.parse_int("9"), None);
        assert_eq!(snow.parse_int("-1"), None);
        assert_eq!(snow.parse_int("x"), None);
        assert_eq!(
            rule("fire_spread_radius_around_player").parse_int("-1"),
            Some(-1)
        );
        assert_eq!(rule("pvp").category, Category::Player);
        assert_eq!(
            rule("max_minecart_speed").feature,
            Some("minecart_improvements")
        );
    }

    #[test]
    fn names_become_language_keys_and_captions() {
        assert_eq!(lang_key("keep_inventory"), "gamerule.keepInventory");
        assert_eq!(lang_key("pvp"), "gamerule.pvp");
        assert_eq!(humanized("max_entity_cramming"), "Max Entity Cramming");
    }

    #[test]
    fn every_rule_has_a_default_and_a_unique_name() {
        let rules = all();
        assert!(rules.len() > 20);
        let mut names: Vec<_> = rules.iter().map(|r| r.name.clone()).collect();
        names.dedup();
        assert_eq!(names.len(), rules.len());
        assert!(
            rules
                .iter()
                .any(|r| r.name == "keep_inventory" && r.default == Value::Bool(false))
        );
    }

    #[test]
    fn changes_survive_a_round_trip_through_the_file() {
        let dir = std::env::temp_dir().join(format!("melon-rules-{}", std::process::id()));
        let mut changes = Changes::new();
        changes.insert(GameRule::KeepInventory.to_string(), Value::Bool(true));
        changes.insert(GameRule::MaxEntityCramming.to_string(), Value::Int(7));
        write_changes(&dir, &changes).unwrap();
        assert_eq!(read_changes(&dir), changes);
        write_changes(&dir, &Changes::new()).unwrap();
        assert!(read_changes(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
