//! The dialog codecs over JSON (vanilla's `MAP_CODEC`s): `None` wherever
//! vanilla's codec fails, so a dialog it would refuse isn't shown either.

use std::sync::Arc;

use serde_json::{Map, Value};

use super::action::{Action, Template, is_variable_name};
use super::body::{DialogBody, ItemBody, PLAIN_MESSAGE_WIDTH, PlainMessage};
use super::input::{
    BooleanInput, Input, InputControl, Multiline, NumberRangeInput, OptionEntry, RangeInfo,
    SingleOptionInput, TextInput,
};
use super::{
    ActionButton, AfterAction, ButtonData, CommonData, DEFAULT_BUTTON_WIDTH, Dialog, DialogKind,
    DialogRef, DialogSet,
};
use melon_protocol::component::{ClickEvent, Component, decode};

type Fields = Map<String, Value>;

/// `Dialog.DIRECT_CODEC`: dispatched on `type`.
pub(super) fn dialog(value: &Value) -> Option<Dialog> {
    let map = value.as_object()?;
    let common = common(map)?;
    let kind = match kind_of(map)? {
        "notice" => DialogKind::Notice {
            action: match map.get("action") {
                Some(action) => action_button(action)?,
                None => ActionButton::plain("gui.ok", DEFAULT_BUTTON_WIDTH),
            },
        },
        "confirmation" => DialogKind::Confirmation {
            yes: action_button(map.get("yes")?)?,
            no: action_button(map.get("no")?)?,
        },
        "multi_action" => {
            let actions = list(map.get("actions")?, action_button)?;
            if actions.is_empty() {
                return None;
            }
            DialogKind::MultiAction {
                actions,
                exit: optional(map, "exit_action", action_button)?,
                columns: positive(map, "columns", 2)?,
            }
        }
        "server_links" => DialogKind::ServerLinks {
            exit: optional(map, "exit_action", action_button)?,
            columns: positive(map, "columns", 2)?,
            button_width: width(map, "button_width", DEFAULT_BUTTON_WIDTH)?,
        },
        "dialog_list" => DialogKind::DialogList {
            dialogs: dialog_set(map.get("dialogs")?)?,
            exit: optional(map, "exit_action", action_button)?,
            columns: positive(map, "columns", 2)?,
            button_width: width(map, "button_width", DEFAULT_BUTTON_WIDTH)?,
        },
        _ => return None,
    };
    Some(Dialog { common, kind })
}

/// `CommonDialogData.MAP_CODEC`, with its check that a pausing dialog
/// unpauses after a button.
fn common(map: &Fields) -> Option<CommonData> {
    let after_action = match map.get("after_action") {
        None => AfterAction::Close,
        Some(value) => match value.as_str()? {
            "close" => AfterAction::Close,
            "none" => AfterAction::None,
            "wait_for_response" => AfterAction::WaitForResponse,
            _ => return None,
        },
    };
    let pause = boolean(map, "pause", true)?;
    if pause && !after_action.will_unpause() {
        return None;
    }
    let body = match map.get("body") {
        None => Vec::new(),
        Some(Value::Array(items)) => items.iter().map(body).collect::<Option<_>>()?,
        Some(single) => vec![body(single)?],
    };
    Some(CommonData {
        title: component(map, "title")?,
        external_title: optional(map, "external_title", |v| Some(decode::component(v)))?,
        can_close_with_escape: boolean(map, "can_close_with_escape", true)?,
        pause,
        after_action,
        body,
        inputs: optional(map, "inputs", |v| list(v, input))?.unwrap_or_default(),
    })
}

/// `DialogBody.DIRECT_CODEC`.
fn body(value: &Value) -> Option<DialogBody> {
    let map = value.as_object()?;
    Some(match kind_of(map)? {
        "plain_message" => DialogBody::PlainMessage(plain_message_fields(map)?),
        "item" => {
            let item = item_template(map.get("item")?)?;
            DialogBody::Item(ItemBody {
                id: item.0,
                count: item.1,
                components: item.2,
                description: optional(map, "description", plain_message)?,
                show_decorations: boolean(map, "show_decorations", true)?,
                show_tooltip: boolean(map, "show_tooltip", true)?,
                width: ranged(map, "width", 16, 1, 256)?,
                height: ranged(map, "height", 16, 1, 256)?,
            })
        }
        _ => return None,
    })
}

/// `PlainMessage.CODEC`: the fields, or a component on its own.
fn plain_message(value: &Value) -> Option<PlainMessage> {
    if let Some(map) = value.as_object()
        && map.contains_key("contents")
    {
        return plain_message_fields(map);
    }
    Some(PlainMessage {
        contents: decode::component(value),
        width: PLAIN_MESSAGE_WIDTH,
    })
}

fn plain_message_fields(map: &Fields) -> Option<PlainMessage> {
    Some(PlainMessage {
        contents: component(map, "contents")?,
        width: width(map, "width", PLAIN_MESSAGE_WIDTH)?,
    })
}

/// `ItemStackTemplate.CODEC`: an id, or the id with a count and components.
fn item_template(value: &Value) -> Option<(String, i32, Option<Value>)> {
    if let Some(id) = value.as_str() {
        return Some((identifier(id), 1, None));
    }
    let map = value.as_object()?;
    let count = match map.get("count") {
        None => 1,
        Some(count) => i32::try_from(count.as_i64()?).ok().filter(|&c| c >= 1)?,
    };
    Some((
        identifier(map.get("id")?.as_str()?),
        count,
        map.get("components").cloned(),
    ))
}

/// `Input.CODEC`: a key and the control's fields.
fn input(value: &Value) -> Option<Input> {
    let map = value.as_object()?;
    let key = map.get("key")?.as_str()?;
    if !is_variable_name(key) {
        return None;
    }
    let control = match kind_of(map)? {
        "text" => {
            let max_length = positive(map, "max_length", 32)?;
            let initial = string_or(map, "initial", "")?;
            if initial.chars().count() > max_length as usize {
                return None;
            }
            InputControl::Text(TextInput {
                width: width(map, "width", 200)?,
                label: component(map, "label")?,
                label_visible: boolean(map, "label_visible", true)?,
                initial,
                max_length,
                multiline: optional(map, "multiline", |v| {
                    let m = v.as_object()?;
                    Some(Multiline {
                        // `ExtraCodecs.POSITIVE_INT`.
                        max_lines: optional(m, "max_lines", |n| {
                            i32::try_from(n.as_i64()?)
                                .ok()
                                .filter(|&n| n > 0)
                                .map(|n| n as u32)
                        })?,
                        height: optional(m, "height", |n| {
                            u32::try_from(n.as_i64()?)
                                .ok()
                                .filter(|n| (1..=512).contains(n))
                        })?,
                    })
                })?,
            })
        }
        "boolean" => InputControl::Boolean(BooleanInput {
            label: component(map, "label")?,
            initial: boolean(map, "initial", false)?,
            on_true: string_or(map, "on_true", "true")?,
            on_false: string_or(map, "on_false", "false")?,
        }),
        "single_option" => {
            let entries = list(map.get("options")?, option_entry)?;
            if entries.is_empty() || entries.iter().filter(|e| e.initial).count() > 1 {
                return None;
            }
            InputControl::SingleOption(SingleOptionInput {
                width: width(map, "width", 200)?,
                entries,
                label: component(map, "label")?,
                label_visible: boolean(map, "label_visible", true)?,
            })
        }
        "number_range" => {
            let range = RangeInfo {
                start: map.get("start")?.as_f64()? as f32,
                end: map.get("end")?.as_f64()? as f32,
                initial: optional(map, "initial", |v| Some(v.as_f64()? as f32))?,
                step: optional(map, "step", |v| {
                    Some(v.as_f64()? as f32).filter(|&s| s > 0.0)
                })?,
            };
            if let Some(initial) = range.initial {
                let (min, max) = (range.start.min(range.end), range.start.max(range.end));
                if initial < min || initial > max {
                    return None;
                }
            }
            InputControl::NumberRange(NumberRangeInput {
                width: width(map, "width", 200)?,
                label: component(map, "label")?,
                label_format: string_or(map, "label_format", "options.generic_value")?,
                range,
            })
        }
        _ => return None,
    };
    Some(Input {
        key: key.to_owned(),
        control,
    })
}

/// `SingleOptionInput.Entry.CODEC`: the fields, or an id on its own.
fn option_entry(value: &Value) -> Option<OptionEntry> {
    if let Some(id) = value.as_str() {
        return Some(OptionEntry {
            id: id.to_owned(),
            display: None,
            initial: false,
        });
    }
    let map = value.as_object()?;
    Some(OptionEntry {
        id: map.get("id")?.as_str()?.to_owned(),
        display: optional(map, "display", |v| Some(decode::component(v)))?,
        initial: boolean(map, "initial", false)?,
    })
}

/// `ActionButton.CODEC`.
fn action_button(value: &Value) -> Option<ActionButton> {
    let map = value.as_object()?;
    Some(ActionButton {
        button: ButtonData {
            label: component(map, "label")?,
            tooltip: optional(map, "tooltip", |v| Some(decode::component(v)))?,
            width: width(map, "width", DEFAULT_BUTTON_WIDTH)?,
        },
        action: optional(map, "action", action)?,
    })
}

/// `Action.CODEC`: dispatched on `type`; the static ones are the click
/// events a server may send, their fields beside `type`.
fn action(value: &Value) -> Option<Action> {
    let map = value.as_object()?;
    Some(match kind_of(map)? {
        "dynamic/run_command" => {
            Action::RunCommand(Template::parse(map.get("template")?.as_str()?)?)
        }
        "dynamic/custom" => Action::Custom {
            id: identifier(map.get("id")?.as_str()?),
            additions: optional(map, "additions", |v| v.as_object().cloned())?,
        },
        kind => {
            let mut fields = map.clone();
            fields.insert("action".into(), Value::String(kind.to_owned()));
            match decode::click_event(&Value::Object(fields))? {
                ClickEvent::OpenFile { .. } => return None,
                event => Action::Static(event),
            }
        }
    })
}

/// `Dialog.LIST_CODEC` (`HolderSet`): `#tag`, one dialog, or a list of them.
fn dialog_set(value: &Value) -> Option<DialogSet> {
    match value {
        Value::String(name) if name.starts_with('#') => {
            Some(DialogSet::Tag(identifier(&name[1..])))
        }
        Value::Array(_) => Some(DialogSet::Each(list(value, dialog_ref)?)),
        single => Some(DialogSet::Each(vec![dialog_ref(single)?])),
    }
}

/// `Dialog.CODEC` (`Holder`): a registry name or a dialog given whole.
pub(super) fn dialog_ref(value: &Value) -> Option<DialogRef> {
    match value {
        Value::String(name) => Some(DialogRef::Named(identifier(name))),
        inline => Some(DialogRef::Inline(Arc::new(dialog(inline)?))),
    }
}

/// The `type` field, its default namespace dropped.
fn kind_of(map: &Fields) -> Option<&str> {
    let kind = map.get("type")?.as_str()?;
    Some(kind.strip_prefix("minecraft:").unwrap_or(kind))
}

/// An id with the default namespace when it has none.
fn identifier(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn component(map: &Fields, key: &str) -> Option<Component> {
    map.get(key).map(decode::component)
}

/// An optional field: `Some(None)` when missing, `None` when present but
/// unreadable (vanilla's `optionalFieldOf` fails on a bad value).
fn optional<T>(map: &Fields, key: &str, read: impl Fn(&Value) -> Option<T>) -> Option<Option<T>> {
    match map.get(key) {
        None => Some(None),
        Some(value) => read(value).map(Some),
    }
}

fn list<T>(value: &Value, read: impl Fn(&Value) -> Option<T>) -> Option<Vec<T>> {
    value.as_array()?.iter().map(read).collect()
}

fn boolean(map: &Fields, key: &str, default: bool) -> Option<bool> {
    match map.get(key) {
        None => Some(default),
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::Number(n)) => Some(n.as_i64()? != 0),
        Some(_) => None,
    }
}

fn string_or(map: &Fields, key: &str, default: &str) -> Option<String> {
    match map.get(key) {
        None => Some(default.to_owned()),
        Some(value) => Some(value.as_str()?.to_owned()),
    }
}

/// An int field in `min..=max`.
fn ranged(map: &Fields, key: &str, default: u32, min: u32, max: u32) -> Option<u32> {
    match map.get(key) {
        None => Some(default),
        Some(value) => u32::try_from(value.as_i64()?)
            .ok()
            .filter(|n| (min..=max).contains(n)),
    }
}

/// `Dialog.WIDTH_CODEC`: 1 to 1024.
fn width(map: &Fields, key: &str, default: u32) -> Option<u32> {
    ranged(map, key, default, 1, 1024)
}

/// `ExtraCodecs.POSITIVE_INT`.
fn positive(map: &Fields, key: &str, default: u32) -> Option<u32> {
    ranged(map, key, default, 1, i32::MAX as u32)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::dialog::DialogClick;

    #[test]
    fn a_full_multi_action_dialog() {
        let value = json!({
            "type": "minecraft:multi_action",
            "title": "Shop",
            "body": {"type": "plain_message", "contents": "Pick one"},
            "inputs": [
                {"type": "text", "key": "name", "label": "Name", "initial": "Steve"},
                {"type": "boolean", "key": "vip", "label": "VIP", "initial": true},
                {"type": "single_option", "key": "size", "label": "Size",
                 "options": ["s", {"id": "m", "initial": true}]},
                {"type": "number_range", "key": "amount", "label": "Amount",
                 "start": 1, "end": 64, "step": 1}
            ],
            "actions": [
                {"label": "Buy", "action": {"type": "dynamic/run_command", "template": "buy $(name) $(amount)"}},
                {"label": "Site", "width": 100, "action": {"type": "open_url", "url": "https://example.com"}},
                {"label": "Send", "action": {"type": "dynamic/custom", "id": "shop:send"}}
            ],
            "columns": 1,
            "pause": false,
            "after_action": "none"
        });
        let dialog = dialog(&value).expect("parses");
        assert_eq!(dialog.common.title.plain_text(), "Shop");
        assert_eq!(dialog.common.body.len(), 1);
        assert_eq!(dialog.common.inputs.len(), 4);
        assert_eq!(dialog.common.after_action, AfterAction::None);
        let DialogKind::MultiAction {
            actions,
            columns,
            exit,
        } = &dialog.kind
        else {
            panic!("multi action");
        };
        assert_eq!((*columns, exit.is_none()), (1, true));
        assert_eq!(actions[1].button.width, 100);
        assert!(matches!(
            actions[1].action,
            Some(Action::Static(ClickEvent::OpenUrl { .. }))
        ));
        let InputControl::SingleOption(size) = &dialog.common.inputs[2].control else {
            panic!("single option");
        };
        assert_eq!(size.initial(), 1);
        let values = vec![("name".into(), crate::dialog::InputValue::string("Alex"))];
        assert_eq!(
            actions[0].action.as_ref().unwrap().resolve(&values),
            DialogClick::Event(ClickEvent::RunCommand {
                command: "buy Alex ".into()
            })
        );
    }

    #[test]
    fn vanilla_refuses_what_is_refused_here() {
        let pausing_and_staying = json!({
            "type": "notice", "title": "x", "after_action": "none"
        });
        assert!(dialog(&pausing_and_staying).is_none());
        let empty_actions = json!({"type": "multi_action", "title": "x", "actions": []});
        assert!(dialog(&empty_actions).is_none());
        let too_wide = json!({"type": "notice", "title": "x",
            "action": {"label": "ok", "width": 2000}});
        assert!(dialog(&too_wide).is_none());
        let unknown = json!({"type": "minecraft:carousel", "title": "x"});
        assert!(dialog(&unknown).is_none());
        let no_variables = json!({"type": "notice", "title": "x",
            "action": {"label": "ok", "action": {"type": "dynamic/run_command", "template": "kill"}}});
        assert!(dialog(&no_variables).is_none());
    }

    #[test]
    fn a_notice_defaults_to_ok_and_lists_take_tags() {
        let notice = dialog(&json!({"type": "notice", "title": "Hi"})).unwrap();
        let DialogKind::Notice { action } = &notice.kind else {
            panic!("notice");
        };
        assert_eq!(action.button.width, DEFAULT_BUTTON_WIDTH);
        let list = dialog(&json!({
            "type": "dialog_list", "title": "More",
            "dialogs": "#minecraft:pause_screen_additions"
        }))
        .unwrap();
        assert!(matches!(
            &list.kind,
            DialogKind::DialogList { dialogs: DialogSet::Tag(tag), .. }
                if tag == "minecraft:pause_screen_additions"
        ));
    }
}
