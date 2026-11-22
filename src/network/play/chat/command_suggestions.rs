//! Completing command arguments on the client, as vanilla's argument types do.

use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::Result;

use super::commands::{Argument, Parser};
use crate::network::protocol::packet::Reader;
use pumpkin_protocol::codec::registry_ids;
use pumpkin_protocol::ser::NetworkReadSliceExt;
use pumpkin_util::version::JavaMinecraftVersion;

const VERSION: JavaMinecraftVersion = JavaMinecraftVersion::V_26_3;

/// `Permissions.COMMANDS_ENTITY_SELECTORS`.
pub const SELECTOR_LEVEL: u8 = 2;

/// The ids of the registries the server sent (`ClientboundRegistryDataPacket`).
#[derive(Clone, Debug, Default)]
pub struct Registries {
    synced: HashMap<String, Vec<String>>,
    /// `ClientPacketListener.serverBrand`, from the configuration phase.
    pub brand: Option<String>,
}

impl Registries {
    /// A `ClientboundCustomPayloadPacket`; only the brand is kept.
    pub fn read_payload(&mut self, body: &[u8]) -> Result<()> {
        let mut r = Reader::new(body);
        if r.string()? == "minecraft:brand" {
            self.brand = Some(r.string()?);
        }
        Ok(())
    }

    /// Records a registry data packet's entry ids.
    pub fn read_packet(&mut self, body: &[u8]) -> Result<()> {
        let mut r = Reader::new(body);
        let registry = r.string()?;
        let count = r.var_int()?.clamp(0, 1 << 16) as usize;
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(r.string()?);
            if r.bool()? {
                r.remaining()
                    .get_nbt(&VERSION)
                    .map_err(|e| anyhow::anyhow!("{registry} NBT: {e}"))?;
            }
        }
        self.synced.insert(registry, ids);
        Ok(())
    }

    /// The synced registries for Pumpkin's codecs, so item components
    /// name the server's own entries (datapacks included) by their ids.
    pub fn codec_registries(&self) -> std::sync::Arc<registry_ids::SyncedRegistries> {
        let mut synced = registry_ids::SyncedRegistries::new();
        for (registry, ids) in &self.synced {
            synced.insert(registry.clone(), ids.clone());
        }
        std::sync::Arc::new(synced)
    }

    /// Resolves item component ids through these registries on this thread.
    pub fn install_for_codecs(&self) {
        registry_ids::install(Some(self.codec_registries()));
    }

    /// A registry's ids, `None` when Melon doesn't know it.
    fn ids(&self, registry: &str) -> Option<&[String]> {
        self.synced
            .get(registry)
            .map(Vec::as_slice)
            .or_else(|| built_in(registry))
    }
}

/// The built-in registries `pumpkin_data` can list.
fn built_in(registry: &str) -> Option<&'static [String]> {
    use pumpkin_data::data_component_impl::IDSetContent;
    static ITEMS: LazyLock<Vec<String>> = LazyLock::new(|| {
        (0..4096)
            .filter_map(pumpkin_data::item::Item::from_id)
            .map(|item| format!("minecraft:{}", item.registry_key))
            .collect()
    });
    static BLOCKS: LazyLock<Vec<String>> = LazyLock::new(|| {
        let mut names: Vec<String> = (0..pumpkin_data::BlockStateId::COUNT)
            .filter_map(pumpkin_data::BlockStateId::new)
            .map(|id| format!("minecraft:{}", pumpkin_data::Block::from_state_id(id).name))
            .collect();
        names.dedup();
        names
    });
    static ENTITY_TYPES: LazyLock<Vec<String>> = LazyLock::new(|| {
        pumpkin_data::entity::EntityType::ALL
            .iter()
            .map(|t| format!("minecraft:{}", t.resource_name))
            .collect()
    });
    static EFFECTS: LazyLock<Vec<String>> = LazyLock::new(|| {
        (0..=u16::from(u8::MAX))
            .filter_map(<pumpkin_data::effect::StatusEffect as IDSetContent>::from_id)
            .map(|e| e.minecraft_name.to_owned())
            .collect()
    });
    static ENCHANTMENTS: LazyLock<Vec<String>> = LazyLock::new(|| {
        pumpkin_data::Enchantment::ALL
            .iter()
            .map(|e| e.name.to_owned())
            .collect()
    });
    let ids: &'static Vec<String> = match registry {
        "minecraft:item" => &ITEMS,
        "minecraft:block" => &BLOCKS,
        "minecraft:entity_type" => &ENTITY_TYPES,
        "minecraft:mob_effect" => &EFFECTS,
        "minecraft:enchantment" => &ENCHANTMENTS,
        _ => return None,
    };
    Some(ids)
}

/// What completing on the client may draw on.
pub struct Source<'a> {
    pub registries: &'a Registries,
    /// The listed players' names (`getOnlinePlayerNames`).
    pub players: &'a [String],
    /// Whether `@` selectors are allowed (permission level 2).
    pub selectors: bool,
}

/// One completion of the word, with its tooltip's translation key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub text: String,
    pub tooltip: Option<&'static str>,
}

impl Suggestion {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tooltip: None,
        }
    }
}

/// `SharedSuggestionProvider.matchesSubStr`.
pub fn matches_sub_str(pattern: &str, input: &str) -> bool {
    let mut index = 0;
    loop {
        if input[index..].starts_with(pattern) {
            return true;
        }
        match input[index..].find(['.', '_', '/']) {
            Some(i) => index += i + 1,
            None => return false,
        }
    }
}

/// `SharedSuggestionProvider.filterResources`.
fn matches_resource(typed: &str, id: &str) -> bool {
    if typed.contains(':') {
        return matches_sub_str(typed, id);
    }
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    matches_sub_str(typed, namespace) || matches_sub_str(typed, path)
}

/// `EntitySelectorParser.fillSelectorSuggestions`.
const SELECTORS: [(&str, &str); 6] = [
    ("@p", "argument.entity.selector.nearestPlayer"),
    ("@a", "argument.entity.selector.allPlayers"),
    ("@r", "argument.entity.selector.randomPlayer"),
    ("@s", "argument.entity.selector.self"),
    ("@e", "argument.entity.selector.allEntities"),
    ("@n", "argument.entity.selector.nearestEntity"),
];

/// `EntityArgument`/`GameProfileArgument.listSuggestions`.
fn entity(word: &str, source: &Source) -> Option<Vec<Suggestion>> {
    let lower = word.to_lowercase();
    let names = || {
        source
            .players
            .iter()
            .filter(|name| matches_sub_str(&lower, &name.to_lowercase()))
            .map(|name| Suggestion::plain(name.clone()))
    };
    let selectors = || {
        SELECTORS.iter().map(|&(text, tooltip)| Suggestion {
            text: text.to_owned(),
            tooltip: Some(tooltip),
        })
    };
    if word.is_empty() {
        let mut out: Vec<_> = names().collect();
        if source.selectors {
            out.extend(selectors());
        }
        return Some(out);
    }
    if let Some(rest) = word.strip_prefix('@') {
        if !source.selectors {
            return Some(Vec::new());
        }
        return match rest.len() {
            0 => Some(selectors().collect()),
            1 if SELECTORS.iter().any(|&(s, _)| s == word) => {
                Some(vec![Suggestion::plain(format!("{word}["))])
            }
            1 => Some(Vec::new()),
            _ => None,
        };
    }
    Some(names().collect())
}

/// Completes `word` as `argument` on the client.
pub fn suggest(argument: &Argument, word: &str, source: &Source) -> Option<Vec<Suggestion>> {
    let lower = word.to_lowercase();
    if let Some(provider) = &argument.suggestions {
        if &**provider != "minecraft:summonable_entities" {
            return None;
        }
        return Some(
            pumpkin_data::entity::EntityType::ALL
                .iter()
                .filter(|t| t.summonable)
                .map(|t| format!("minecraft:{}", t.resource_name))
                .filter(|id| matches_resource(&lower, id))
                .map(Suggestion::plain)
                .collect(),
        );
    }
    match &argument.parser {
        Parser::Bool => Some(
            ["true", "false"]
                .into_iter()
                .filter(|v| v.starts_with(&lower))
                .map(Suggestion::plain)
                .collect(),
        ),
        Parser::GameMode => Some(
            ["survival", "creative", "adventure", "spectator"]
                .into_iter()
                .filter(|v| matches_sub_str(&lower, v))
                .map(Suggestion::plain)
                .collect(),
        ),
        Parser::Entity { .. } | Parser::GameProfile => entity(word, source),
        Parser::Registry { registry, tags } => {
            if lower.starts_with('#') {
                return if *tags { None } else { Some(Vec::new()) };
            }
            let ids = source.registries.ids(registry)?;
            Some(
                ids.iter()
                    .filter(|id| matches_resource(&lower, id))
                    .map(|id| Suggestion::plain(id.clone()))
                    .collect(),
            )
        }
        Parser::Other(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argument(parser: Parser) -> Argument {
        Argument {
            name: "x".into(),
            parser,
            parser_id: -1,
            greedy: false,
            words: 1,
            suggestions: None,
        }
    }

    fn texts(suggestions: Option<Vec<Suggestion>>) -> Vec<String> {
        suggestions.unwrap().into_iter().map(|s| s.text).collect()
    }

    #[test]
    fn sub_strings_match_after_separators() {
        assert!(matches_sub_str("eff", "efficiency"));
        assert!(matches_sub_str("prot", "fire_protection"));
        assert!(matches_sub_str("", "anything"));
        assert!(!matches_sub_str("ency", "efficiency"));
        assert!(matches_resource("eff", "minecraft:efficiency"));
        assert!(matches_resource("mine", "minecraft:efficiency"));
        assert!(matches_resource("minecraft:eff", "minecraft:efficiency"));
        assert!(!matches_resource("minecraft:prot", "minecraft:efficiency"));
    }

    #[test]
    fn enchantments_complete_with_one_namespace() {
        let registries = Registries::default();
        let source = Source {
            registries: &registries,
            players: &[],
            selectors: true,
        };
        let enchantment = argument(Parser::Registry {
            registry: "minecraft:enchantment".into(),
            tags: false,
        });
        let found = texts(suggest(&enchantment, "minecraft:effi", &source));
        assert_eq!(found, ["minecraft:efficiency"]);
        assert_eq!(
            texts(suggest(&enchantment, "effi", &source)),
            ["minecraft:efficiency"]
        );
        let unknown = argument(Parser::Registry {
            registry: "minecraft:loot_table".into(),
            tags: false,
        });
        assert_eq!(suggest(&unknown, "", &source), None);
    }

    #[test]
    fn synced_registries_win_over_built_in_ones() {
        let mut registries = Registries::default();
        registries.synced.insert(
            "minecraft:enchantment".to_owned(),
            vec!["minecraft:efficiency".to_owned(), "custom:zap".to_owned()],
        );
        let source = Source {
            registries: &registries,
            players: &[],
            selectors: true,
        };
        let enchantment = argument(Parser::Registry {
            registry: "minecraft:enchantment".into(),
            tags: true,
        });
        assert_eq!(texts(suggest(&enchantment, "z", &source)), ["custom:zap"]);
        assert_eq!(suggest(&enchantment, "#", &source), None);
    }

    #[test]
    fn entities_complete_names_and_selectors() {
        let registries = Registries::default();
        let players = ["Steve".to_owned(), "Alex".to_owned()];
        let mut source = Source {
            registries: &registries,
            players: &players,
            selectors: true,
        };
        let target = argument(Parser::Entity {
            players_only: false,
        });
        let all = suggest(&target, "", &source).unwrap();
        assert_eq!(all.len(), 2 + 6);
        assert_eq!(texts(suggest(&target, "st", &source)), ["Steve"]);
        assert_eq!(texts(suggest(&target, "@", &source)).len(), 6);
        assert_eq!(texts(suggest(&target, "@a", &source)), ["@a["]);
        assert_eq!(suggest(&target, "@a[l", &source), None);
        source.selectors = false;
        assert_eq!(texts(suggest(&target, "", &source)), ["Steve", "Alex"]);
        assert!(texts(suggest(&target, "@", &source)).is_empty());
    }

    #[test]
    fn only_ask_server_arguments_go_to_the_server() {
        let registries = Registries::default();
        let source = Source {
            registries: &registries,
            players: &[],
            selectors: true,
        };
        let mut asked = argument(Parser::Bool);
        asked.suggestions = Some("minecraft:ask_server".into());
        assert_eq!(suggest(&asked, "", &source), None);
        assert_eq!(
            texts(suggest(&argument(Parser::Bool), "t", &source)),
            ["true"]
        );
        assert_eq!(
            texts(suggest(&argument(Parser::GameMode), "c", &source)),
            ["creative"]
        );
        assert_eq!(suggest(&argument(Parser::Other(8)), "", &source), None);
        let mut summon = argument(Parser::Registry {
            registry: "minecraft:entity_type".into(),
            tags: false,
        });
        summon.suggestions = Some("minecraft:summonable_entities".into());
        let found = texts(suggest(&summon, "zomb", &source));
        assert!(found.contains(&"minecraft:zombie".to_owned()), "{found:?}");
        assert!(!found.iter().any(|id| id.contains("player")));
    }
}
