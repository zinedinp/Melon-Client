//! `ComponentSerialization.CODEC`.

use serde_json::{Map, Value};
use uuid::Uuid;

use super::{
    AtlasSprite, ClickEvent, Component, Contents, FontDescription, HoverEvent, NbtSource,
    ObjectInfo, PlayerSprite, Profile, Style, TextColor,
};

/// The component `value` holds; unreadable ones are empty text.
pub fn component(value: &Value) -> Component {
    match value {
        Value::String(text) => Component::text(text.as_str()),
        Value::Number(n) => Component::text(n.to_string()),
        Value::Bool(b) => Component::text(b.to_string()),
        Value::Array(list) => {
            let mut parts = list.iter().map(component);
            let mut first = parts.next().unwrap_or_default();
            first.extra.extend(parts);
            first
        }
        Value::Object(map) => Component {
            contents: Box::new(contents(map)),
            style: Box::new(style(map)),
            extra: components_field(map, "extra"),
        },
        Value::Null => Component::empty(),
    }
}

/// A style on its own (its keys are a component's).
pub fn style_of(value: &Value) -> Style {
    value.as_object().map(style).unwrap_or_default()
}

fn string(map: &Map<String, Value>, key: &str) -> Option<String> {
    match map.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `Codec.BOOL`: NBT writes it as a byte.
fn boolean(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => Some(n.as_i64()? != 0),
        _ => None,
    }
}

fn flag(map: &Map<String, Value>, key: &str) -> Option<bool> {
    map.get(key).and_then(boolean)
}

fn optional_component(map: &Map<String, Value>, key: &str) -> Option<Box<Component>> {
    map.get(key).map(|v| Box::new(component(v)))
}

/// An `Identifier` with its namespace.
fn identifier(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn contents(map: &Map<String, Value>) -> Contents {
    if let Some(Value::String(kind)) = map.get("type") {
        let kind = kind.strip_prefix("minecraft:").unwrap_or(kind);
        let found = match kind {
            "text" => text(map),
            "translatable" => translatable(map),
            "keybind" => keybind(map),
            "score" => score(map),
            "selector" => selector(map),
            "nbt" => nbt(map),
            "object" => object(map),
            _ => None,
        };
        if let Some(found) = found {
            return found;
        }
    }
    text(map)
        .or_else(|| translatable(map))
        .or_else(|| keybind(map))
        .or_else(|| score(map))
        .or_else(|| selector(map))
        .or_else(|| nbt(map))
        .or_else(|| object(map))
        .unwrap_or_else(|| Contents::Text(String::new()))
}

fn text(map: &Map<String, Value>) -> Option<Contents> {
    string(map, "text").map(Contents::Text)
}

fn translatable(map: &Map<String, Value>) -> Option<Contents> {
    let key = string(map, "translate")?;
    Some(Contents::Translatable {
        key,
        fallback: string(map, "fallback"),
        args: components_field(map, "with"),
    })
}

/// `extra` / `with`.
fn components_field(map: &Map<String, Value>, key: &str) -> Vec<Component> {
    match map.get(key) {
        Some(Value::Array(list)) => list.iter().map(component).collect(),
        Some(Value::Null) | None => Vec::new(),
        Some(value) => vec![component(value)],
    }
}

fn keybind(map: &Map<String, Value>) -> Option<Contents> {
    string(map, "keybind").map(Contents::Keybind)
}

fn score(map: &Map<String, Value>) -> Option<Contents> {
    let inner = map.get("score")?.as_object()?;
    Some(Contents::Score {
        name: string(inner, "name")?,
        objective: string(inner, "objective")?,
    })
}

fn selector(map: &Map<String, Value>) -> Option<Contents> {
    Some(Contents::Selector {
        selector: string(map, "selector")?,
        separator: optional_component(map, "separator"),
    })
}

fn nbt(map: &Map<String, Value>) -> Option<Contents> {
    let path = string(map, "nbt")?;
    let kind = string(map, "source");
    let pick = |name: &str| string(map, name);
    let source = match kind.as_deref() {
        Some("block") => NbtSource::Block(pick("block")?),
        Some("entity") => NbtSource::Entity(pick("entity")?),
        Some("storage") => NbtSource::Storage(pick("storage")?),
        _ => pick("block")
            .map(NbtSource::Block)
            .or_else(|| pick("entity").map(NbtSource::Entity))
            .or_else(|| pick("storage").map(NbtSource::Storage))?,
    };
    let interpreting = flag(map, "interpret").unwrap_or(false);
    let plain = flag(map, "plain").unwrap_or(false);
    if interpreting && plain {
        return None;
    }
    Some(Contents::Nbt {
        path,
        interpreting,
        plain,
        separator: optional_component(map, "separator"),
        source,
    })
}

fn object(map: &Map<String, Value>) -> Option<Contents> {
    let kind = string(map, "object")
        .map(|kind| kind.strip_prefix("minecraft:").unwrap_or(&kind).to_owned());
    let info = match kind.as_deref() {
        Some("atlas") => atlas(map).unwrap_or_else(missing_atlas),
        Some("player") => player(map).unwrap_or_else(missing_player),
        Some(_) => atlas(map)
            .or_else(|| player(map))
            .unwrap_or_else(missing_atlas),
        None => atlas(map).or_else(|| player(map))?,
    };
    Some(Contents::Object {
        info,
        fallback: optional_component(map, "fallback"),
    })
}

fn missing_atlas() -> ObjectInfo {
    ObjectInfo::Atlas(AtlasSprite {
        atlas: ObjectInfo::DEFAULT_ATLAS.to_owned(),
        sprite: "minecraft:missingno".into(),
    })
}

fn missing_player() -> ObjectInfo {
    ObjectInfo::Player(PlayerSprite {
        profile: Profile::default(),
        hat: true,
    })
}

fn atlas(map: &Map<String, Value>) -> Option<ObjectInfo> {
    let sprite = identifier(&string(map, "sprite")?);
    let atlas = string(map, "atlas").map_or_else(
        || ObjectInfo::DEFAULT_ATLAS.to_owned(),
        |atlas| identifier(&atlas),
    );
    Some(ObjectInfo::Atlas(AtlasSprite { atlas, sprite }))
}

fn player(map: &Map<String, Value>) -> Option<ObjectInfo> {
    let profile = profile(map.get("player")?)?;
    Some(ObjectInfo::Player(PlayerSprite {
        profile,
        hat: flag(map, "hat").unwrap_or(true),
    }))
}

/// `ResolvableProfile.CODEC`.
fn profile(value: &Value) -> Option<Profile> {
    match value {
        Value::String(name) => Some(Profile {
            name: Some(name.clone()),
            ..Profile::default()
        }),
        Value::Object(map) => {
            let textures = match map.get("properties") {
                Some(Value::Array(list)) => list.iter().find_map(|p| {
                    let p = p.as_object()?;
                    (string(p, "name")? == "textures").then(|| string(p, "value"))?
                }),
                _ => None,
            };
            Some(Profile {
                name: string(map, "name"),
                id: map.get("id").and_then(uuid),
                textures,
                texture: string(map, "texture").map(|t| identifier(&t)),
            })
        }
        _ => None,
    }
}

/// `UUIDUtil.LENIENT_CODEC`: a string or four ints.
fn uuid(value: &Value) -> Option<Uuid> {
    match value {
        Value::String(s) => Uuid::parse_str(s).ok(),
        Value::Array(parts) if parts.len() == 4 => {
            let mut bits = [0u8; 16];
            for (i, part) in parts.iter().enumerate() {
                let n = part.as_i64()? as i32;
                bits[i * 4..i * 4 + 4].copy_from_slice(&n.to_be_bytes());
            }
            Some(Uuid::from_bytes(bits))
        }
        _ => None,
    }
}

fn style(map: &Map<String, Value>) -> Style {
    Style {
        color: map.get("color").and_then(|c| match c {
            Value::String(s) => TextColor::parse(s),
            Value::Number(n) => n
                .as_i64()
                .map(|v| TextColor([(v >> 16) as u8, (v >> 8) as u8, v as u8])),
            _ => None,
        }),
        shadow_color: map.get("shadow_color").and_then(shadow_color),
        bold: flag(map, "bold"),
        italic: flag(map, "italic"),
        underlined: flag(map, "underlined"),
        strikethrough: flag(map, "strikethrough"),
        obfuscated: flag(map, "obfuscated"),
        click_event: map.get("click_event").and_then(click),
        hover_event: map.get("hover_event").and_then(hover),
        insertion: string(map, "insertion"),
        font: string(map, "font").map(|font| FontDescription(identifier(&font))),
    }
}

/// `ExtraCodecs.ARGB_COLOR_CODEC`: an ARGB int, or four floats.
fn shadow_color(value: &Value) -> Option<[u8; 4]> {
    match value {
        Value::Number(n) => {
            let v = n.as_i64()? as u32;
            Some([(v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8])
        }
        Value::Array(parts) if parts.len() == 4 => {
            let mut out = [0u8; 4];
            for (slot, part) in out.iter_mut().zip(parts) {
                *slot = (part.as_f64()?.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            Some(out)
        }
        _ => None,
    }
}

/// `ClickEvent.CODEC`, the actions a server may send: `action` and the
/// action's fields.
pub fn click_event(value: &Value) -> Option<ClickEvent> {
    click(value)
}

/// `ClickEvent.CODEC`, the actions a server may send.
fn click(value: &Value) -> Option<ClickEvent> {
    let map = value.as_object()?;
    Some(match string(map, "action")?.as_str() {
        // `ExtraCodecs.UNTRUSTED_URI`: only http(s) links are kept.
        "open_url" => ClickEvent::OpenUrl {
            url: string(map, "url").filter(|url| super::is_trusted_url(url))?,
        },
        "run_command" => ClickEvent::RunCommand {
            command: string(map, "command")?,
        },
        "suggest_command" => ClickEvent::SuggestCommand {
            command: string(map, "command")?,
        },
        "change_page" => ClickEvent::ChangePage {
            page: map.get("page")?.as_i64()? as i32,
        },
        "copy_to_clipboard" => ClickEvent::CopyToClipboard {
            value: string(map, "value")?,
        },
        "show_dialog" => ClickEvent::ShowDialog {
            dialog: map.get("dialog")?.clone(),
        },
        "custom" => ClickEvent::Custom {
            id: identifier(&string(map, "id")?),
            payload: map.get("payload").cloned(),
        },
        _ => return None,
    })
}

/// `HoverEvent.CODEC`.
fn hover(value: &Value) -> Option<HoverEvent> {
    let map = value.as_object()?;
    Some(match string(map, "action")?.as_str() {
        "show_text" => {
            let text = map.get("value").or_else(|| map.get("contents"))?;
            HoverEvent::ShowText(Box::new(component(text)))
        }
        "show_item" => {
            let inner = map.get("contents").and_then(Value::as_object);
            HoverEvent::ShowItem {
                id: string(map, "id")
                    .or_else(|| inner.and_then(|inner| string(inner, "id")))
                    .or_else(|| match map.get("contents") {
                        Some(Value::String(s)) => Some(s.clone()),
                        _ => None,
                    })?,
                count: map
                    .get("count")
                    .or_else(|| inner.and_then(|inner| inner.get("count")))
                    .and_then(Value::as_i64)
                    .unwrap_or(1) as i32,
                components: map
                    .get("components")
                    .cloned()
                    .or_else(|| inner.and_then(|inner| inner.get("components").cloned())),
            }
        }
        "show_entity" => HoverEvent::ShowEntity {
            id: string(map, "id")?,
            uuid: map.get("uuid").and_then(uuid),
            name: optional_component(map, "name"),
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_string_is_text_and_a_list_is_its_first_with_siblings() {
        assert_eq!(component(&json!("hi")), Component::text("hi"));
        let list = component(&json!(["a", {"text": "b", "bold": 1}, "c"]));
        assert_eq!(*list.contents, Contents::Text("a".into()));
        assert_eq!(list.extra.len(), 2);
        assert_eq!(list.extra[0].style.bold, Some(true));
        assert_eq!(list.plain_text(), "abc");
    }

    #[test]
    fn contents_are_told_by_their_keys_or_by_type() {
        assert!(matches!(
            *component(&json!({"keybind": "key.jump"})).contents,
            Contents::Keybind(_)
        ));
        assert!(matches!(
            *component(&json!({"type": "selector", "selector": "@p"})).contents,
            Contents::Selector { .. }
        ));
        assert!(matches!(
            *component(&json!({"score": {"name": "a", "objective": "b"}})).contents,
            Contents::Score { .. }
        ));
        assert_eq!(
            *component(&json!({"nbt": "a.b", "storage": "x:y"})).contents,
            Contents::Nbt {
                path: "a.b".into(),
                interpreting: false,
                plain: false,
                separator: None,
                source: NbtSource::Storage("x:y".into()),
            }
        );
        assert_eq!(
            *component(&json!({"nbt": "a", "storage": "x:y", "interpret": 1, "plain": 1})).contents,
            Contents::Text(String::new())
        );
    }

    #[test]
    fn a_translation_keeps_its_fallback_and_reads_plain_arguments() {
        let c = component(
            &json!({"translate": "k", "fallback": "Hallo %s", "with": ["x", 3, true, {"text": "y"}]}),
        );
        let Contents::Translatable {
            key,
            fallback,
            args,
        } = *c.contents
        else {
            panic!("translatable");
        };
        assert_eq!(key, "k");
        assert_eq!(fallback.as_deref(), Some("Hallo %s"));
        assert_eq!(
            args.iter().map(Component::plain_text).collect::<Vec<_>>(),
            ["x", "3", "true", "y"]
        );
    }

    #[test]
    fn objects_are_sprites_or_heads() {
        let atlas = component(&json!({"object": "atlas", "sprite": "block/stone"}));
        assert_eq!(
            *atlas.contents,
            Contents::Object {
                info: ObjectInfo::Atlas(AtlasSprite {
                    atlas: "minecraft:blocks".into(),
                    sprite: "minecraft:block/stone".into(),
                }),
                fallback: None,
            }
        );
        assert_eq!(atlas.plain_text(), "[block/stone]");
        let other =
            component(&json!({"atlas": "items", "sprite": "item/apple", "fallback": "apple"}));
        assert_eq!(other.plain_text(), "apple");
        let head = component(&json!({"object": "player", "player": {"name": "Alex"}, "hat": 0}));
        let Contents::Object {
            info: ObjectInfo::Player(p),
            ..
        } = &*head.contents
        else {
            panic!("player");
        };
        assert_eq!(p.profile.name.as_deref(), Some("Alex"));
        assert!(!p.hat);
        assert_eq!(head.plain_text(), "[Alex head]");
        let named = component(&json!({"object": "player", "player": "Steve"}));
        assert_eq!(named.plain_text(), "[Steve head]");
    }

    #[test]
    fn links_other_than_http_are_dropped() {
        for url in ["file:///C:/Windows", "ms-settings:", "javascript:alert(1)"] {
            let c =
                component(&json!({"text": "x", "click_event": {"action": "open_url", "url": url}}));
            assert_eq!(c.style.click_event, None, "{url}");
        }
    }

    #[test]
    fn a_style_reads_colours_flags_and_events() {
        let c = component(&json!({
            "text": "x", "color": "#FF8800", "bold": 1, "italic": 0,
            "shadow_color": [1.0, 0.0, 0.0, 0.5],
            "insertion": "ins", "font": "minecraft:alt",
            "click_event": {"action": "open_url", "url": "https://a.b"},
            "hover_event": {"action": "show_text", "value": "tip"}
        }));
        let s = &c.style;
        assert_eq!(s.color, Some(TextColor([255, 136, 0])));
        assert_eq!((s.bold, s.italic), (Some(true), Some(false)));
        assert_eq!(s.shadow_color, Some([255, 0, 0, 128]));
        assert_eq!(s.insertion.as_deref(), Some("ins"));
        assert_eq!(s.font, Some(FontDescription("minecraft:alt".into())));
        assert_eq!(
            s.click_event,
            Some(ClickEvent::OpenUrl {
                url: "https://a.b".into()
            })
        );
        assert!(matches!(&s.hover_event, Some(HoverEvent::ShowText(t)) if t.plain_text() == "tip"));
        assert_eq!(
            component(&json!({"text": "", "color": "red"})).style.color,
            Some(TextColor::RED)
        );
        assert_eq!(
            component(&json!({"text": "", "shadow_color": -16_777_216}))
                .style
                .shadow_color,
            Some([0, 0, 0, 255])
        );
    }

    #[test]
    fn what_cannot_be_read_is_left_out() {
        assert_eq!(component(&json!({"unknown": 1})).plain_text(), "");
        assert_eq!(component(&json!(null)).plain_text(), "");
        assert!(
            component(&json!({"text": "a", "click_event": {"action": "open_file", "path": "x"}}))
                .style
                .click_event
                .is_none()
        );
        assert_eq!(
            component(&json!({"text": "a", "color": "nope"}))
                .style
                .color,
            None
        );
    }

    #[test]
    fn extra_and_with_accept_a_single_sibling() {
        let extra = component(&json!({"text": "a", "extra": {"text": "b", "bold": 1}}));
        assert_eq!(extra.plain_text(), "ab");
        assert_eq!(extra.extra[0].style.bold, Some(true));
        let with = component(&json!({"translate": "k", "with": "x"}));
        let Contents::Translatable { args, .. } = &*with.contents else {
            panic!("translatable");
        };
        assert_eq!(args[0].plain_text(), "x");
    }

    #[test]
    fn hover_show_text_accepts_contents_or_value() {
        let value = component(&json!({
            "text": "a",
            "hover_event": {"action": "show_text", "contents": "tip"}
        }));
        assert!(
            matches!(&value.style.hover_event, Some(HoverEvent::ShowText(t)) if t.plain_text() == "tip")
        );
    }

    #[test]
    fn an_object_type_still_renders_when_its_sprite_is_thin() {
        let c = component(&json!({"type": "object", "object": "atlas"}));
        assert!(matches!(*c.contents, Contents::Object { .. }));
        assert!(!c.plain_text().is_empty());
    }

    #[test]
    fn a_font_id_gets_the_default_namespace() {
        assert_eq!(
            component(&json!({"text": "a", "font": "alt"})).style.font,
            Some(FontDescription("minecraft:alt".into()))
        );
    }

    #[test]
    fn an_object_kind_may_be_namespaced() {
        let c = component(&json!({
            "object": "minecraft:atlas",
            "sprite": "item/apple"
        }));
        assert_eq!(
            *c.contents,
            Contents::Object {
                info: ObjectInfo::Atlas(AtlasSprite {
                    atlas: "minecraft:blocks".into(),
                    sprite: "minecraft:item/apple".into(),
                }),
                fallback: None,
            }
        );
    }

    #[test]
    fn hover_show_item_reads_an_item_stack_template() {
        let hover = component(&json!({
            "text": "a",
            "hover_event": {
                "action": "show_item",
                "contents": {"id": "minecraft:stone", "count": 3}
            }
        }));
        assert!(matches!(
            &hover.style.hover_event,
            Some(HoverEvent::ShowItem { id, count, .. })
                if id == "minecraft:stone" && *count == 3
        ));
        let bare = component(&json!({
            "text": "a",
            "hover_event": {"action": "show_item", "id": "minecraft:dirt"}
        }));
        assert!(matches!(
            &bare.style.hover_event,
            Some(HoverEvent::ShowItem { id, count, .. })
                if id == "minecraft:dirt" && *count == 1
        ));
    }
}
