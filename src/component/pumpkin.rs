//! The one place Melon meets Pumpkin's `TextComponent`.

use pumpkin_util::text::TextComponent;
use serde_json::{Map, Value};

use super::{Component, Contents, decode};

/// A component of a stack's data, as a [`Component`].
pub fn from_pumpkin(component: &TextComponent) -> Component {
    serde_json::to_value(component)
        .map(|json| decode::component(&json))
        .unwrap_or_else(|_| Component::text(component.clone().get_text()))
}

/// A [`Component`] as the stack's data holds it.
pub fn to_pumpkin(component: &Component) -> TextComponent {
    serde_json::from_value(json(component))
        .unwrap_or_else(|_| TextComponent::text(component.plain_text()))
}

fn json(component: &Component) -> Value {
    let mut map = Map::new();
    match &*component.contents {
        Contents::Text(text) => {
            map.insert("text".into(), text.as_str().into());
        }
        Contents::Translatable { key, args, .. } => {
            map.insert("translate".into(), key.as_str().into());
            if !args.is_empty() {
                map.insert("with".into(), args.iter().map(json).collect());
            }
        }
        Contents::Keybind(key) => {
            map.insert("keybind".into(), key.as_str().into());
        }
        Contents::Selector { selector, .. } => {
            map.insert("selector".into(), selector.as_str().into());
        }
        other => {
            let shown = Component::new(other.clone()).plain_text();
            map.insert("text".into(), shown.into());
        }
    }
    let style = &component.style;
    if let Some(color) = style.color {
        let [r, g, b] = color.0;
        map.insert("color".into(), format!("#{r:02X}{g:02X}{b:02X}").into());
    }
    for (name, on) in [
        ("bold", style.bold),
        ("italic", style.italic),
        ("underlined", style.underlined),
        ("strikethrough", style.strikethrough),
        ("obfuscated", style.obfuscated),
    ] {
        if let Some(on) = on {
            map.insert(name.into(), on.into());
        }
    }
    if !component.extra.is_empty() {
        map.insert("extra".into(), component.extra.iter().map(json).collect());
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_keeps_its_text_colour_and_flags() {
        let component = decode::component(&serde_json::json!(
            {"text": "Sword", "color": "gold", "italic": false,
             "extra": [{"text": "!", "bold": true}]}
        ));
        let back = from_pumpkin(&to_pumpkin(&component));
        assert_eq!(back.plain_text(), "Sword!");
        assert_eq!(back.style.color, component.style.color);
        assert_eq!(back.style.italic, Some(false));
        assert_eq!(back.extra[0].style.bold, Some(true));
    }
}
