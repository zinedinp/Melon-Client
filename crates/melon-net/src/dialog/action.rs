//! What a dialog's button does (vanilla `server/dialog/action`): a fixed
//! click event, a command built from the inputs, or the inputs sent to the
//! server as a custom payload.

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use serde_json::Value;

use melon_protocol::component::ClickEvent;

/// `Action.ValueGetter`: an input's value, as a command template takes it
/// and as a payload carries it.
#[derive(Clone, Debug, PartialEq)]
pub struct InputValue {
    /// `asTemplateSubstitution`.
    pub text: String,
    /// `asTag`.
    pub tag: NbtTag,
}

impl InputValue {
    /// `ValueGetter.of(String)`: the same string both ways.
    pub fn string(value: impl Into<String>) -> Self {
        let text = value.into();
        Self {
            tag: NbtTag::String(text.clone().into()),
            text,
        }
    }
}

/// `Action`'s kinds (`ActionTypes`).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// `StaticAction`: a click event as it is.
    Static(ClickEvent),
    /// `CommandTemplate` (`dynamic/run_command`): the inputs put into a
    /// command.
    RunCommand(Template),
    /// `CustomAll` (`dynamic/custom`): every input, over `additions`, sent
    /// under `id`.
    Custom {
        id: String,
        additions: Option<serde_json::Map<String, Value>>,
    },
}

/// What a button asks for once its action has the inputs' values.
#[derive(Clone, Debug)]
pub enum DialogClick {
    Event(ClickEvent),
    /// `ClickEvent.Custom` with an NBT payload: `ServerboundCustomClickAction`.
    Custom {
        id: String,
        payload: Option<NbtTag>,
    },
    /// `ClickEvent.ShowDialog` with the dialog at hand (a `dialog_list`'s).
    Show(std::sync::Arc<super::Dialog>),
}

impl PartialEq for DialogClick {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Event(a), Self::Event(b)) => a == b,
            (
                Self::Custom { id, payload },
                Self::Custom {
                    id: other_id,
                    payload: other_payload,
                },
            ) => id == other_id && payload == other_payload,
            (Self::Show(a), Self::Show(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl DialogClick {
    /// A click event, its custom payload (JSON, as components carry it)
    /// turned to NBT.
    pub fn from_event(event: ClickEvent) -> Self {
        match event {
            ClickEvent::Custom { id, payload } => Self::Custom {
                id,
                payload: payload.as_ref().map(json_to_nbt),
            },
            event => Self::Event(event),
        }
    }
}

impl Action {
    /// `Action.createAction`: the click with the inputs' values, by key in
    /// the order the dialog declares them.
    pub fn resolve(&self, values: &[(String, InputValue)]) -> DialogClick {
        match self {
            Self::Static(event) => DialogClick::from_event(event.clone()),
            Self::RunCommand(template) => {
                let command = template.instantiate(|name| {
                    values
                        .iter()
                        .rev()
                        .find(|(key, _)| key == name)
                        .map(|(_, value)| value.text.clone())
                        .unwrap_or_default()
                });
                DialogClick::Event(ClickEvent::RunCommand { command })
            }
            Self::Custom { id, additions } => {
                let mut tag = match additions {
                    Some(map) => match json_to_nbt(&Value::Object(map.clone())) {
                        NbtTag::Compound(compound) => compound,
                        _ => NbtCompound::new(),
                    },
                    None => NbtCompound::new(),
                };
                for (key, value) in values {
                    tag.put(key, value.tag.clone());
                }
                DialogClick::Custom {
                    id: id.clone(),
                    payload: Some(NbtTag::Compound(tag)),
                }
            }
        }
    }
}

/// `StringTemplate`: text with `$(name)` holes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    segments: Vec<String>,
    variables: Vec<String>,
}

impl Template {
    /// `StringTemplate.fromString`: `None` for an unterminated or invalid
    /// variable, or for text with no variable at all, as vanilla refuses.
    pub fn parse(input: &str) -> Option<Self> {
        let mut segments = Vec::new();
        let mut variables = Vec::new();
        let mut start = 0;
        let mut search = 0;
        while let Some(found) = input[search..].find('$') {
            let index = search + found;
            if input[index + 1..].starts_with('(') {
                segments.push(input[start..index].to_owned());
                let end = index + 1 + input[index + 1..].find(')')?;
                let variable = &input[index + 2..end];
                if !is_variable_name(variable) {
                    return None;
                }
                variables.push(variable.to_owned());
                start = end + 1;
                search = start;
            } else {
                search = index + 1;
            }
        }
        if start == 0 {
            return None;
        }
        if start != input.len() {
            segments.push(input[start..].to_owned());
        }
        Some(Self {
            segments,
            variables,
        })
    }

    /// `ParsedTemplate.instantiate`: each hole filled with its variable's
    /// value (empty for an unknown one).
    pub fn instantiate(&self, value: impl Fn(&str) -> String) -> String {
        let mut out = String::new();
        for (segment, variable) in self.segments.iter().zip(&self.variables) {
            out.push_str(segment);
            out.push_str(&value(variable));
        }
        if self.segments.len() > self.variables.len()
            && let Some(last) = self.segments.last()
        {
            out.push_str(last);
        }
        out
    }
}

/// `StringTemplate.isValidVariableName`: letters, digits and `_`.
pub fn is_variable_name(name: &str) -> bool {
    name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// `StringTag.escapeWithoutQuotes`: quotes and backslashes escaped, and
/// control characters as SNBT writes them.
pub fn escape_without_quotes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '"' | '\'' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// JSON (as NBT reads into it) back to NBT: booleans as bytes, whole numbers
/// as ints (longs past that), others as doubles.
pub fn json_to_nbt(value: &Value) -> NbtTag {
    match value {
        Value::Null => NbtTag::End,
        Value::Bool(b) => NbtTag::Byte(i8::from(*b)),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i32::try_from(i).map_or(NbtTag::Long(i), NbtTag::Int),
            None => NbtTag::Double(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => NbtTag::String(s.as_str().into()),
        Value::Array(list) => NbtTag::List(list.iter().map(json_to_nbt).collect()),
        Value::Object(map) => {
            let mut compound = NbtCompound::new();
            for (key, value) in map {
                compound.put(key, json_to_nbt(value));
            }
            NbtTag::Compound(compound)
        }
    }
}

/// `ByteBufCodecs.optionalTagCodec` inside `lengthPrefixed(65536)`, as
/// `ServerboundCustomClickActionPacket` writes its payload: the length, then
/// the nameless tag (`End` for none). `None` past the limit.
pub fn write_payload(out: &mut Vec<u8>, payload: Option<&NbtTag>) -> Option<()> {
    let mut tag = Vec::new();
    match payload {
        Some(payload) => {
            let mut writer = pumpkin_nbt::serializer::NbtWriteHelperJava::new(&mut tag);
            payload.clone().serialize(&mut writer).ok()?;
        }
        None => tag.push(0),
    }
    if tag.len() > 65536 {
        return None;
    }
    melon_protocol::varint::write_var_int(out, tag.len() as i32).ok()?;
    out.extend_from_slice(&tag);
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_fill_their_holes_and_need_one() {
        let template = Template::parse("say $(name) is $(age)!").unwrap();
        let filled = template.instantiate(|name| match name {
            "name" => "Steve".to_owned(),
            _ => String::new(),
        });
        assert_eq!(filled, "say Steve is !");
        assert_eq!(
            Template::parse("$(a)$(b)")
                .unwrap()
                .instantiate(|n| n.to_uppercase()),
            "AB"
        );
        assert!(Template::parse("say hi").is_none(), "no variables");
        assert!(Template::parse("say $(oops").is_none(), "unterminated");
        assert!(Template::parse("say $(no-dash)").is_none(), "invalid name");
        assert_eq!(
            Template::parse("cost $5 $(x)")
                .unwrap()
                .instantiate(|_| "1".into()),
            "cost $5 1"
        );
    }

    #[test]
    fn escaping_matches_snbt() {
        assert_eq!(escape_without_quotes(r#"a"b'c\d"#), r#"a\"b\'c\\d"#);
        assert_eq!(escape_without_quotes("x\ny\u{1}"), "x\\ny\\x01");
    }

    #[test]
    fn custom_actions_put_every_input_over_the_additions() {
        let additions = serde_json::json!({"page": 2, "flag": true});
        let action = Action::Custom {
            id: "test:submit".into(),
            additions: additions.as_object().cloned(),
        };
        let values = vec![
            ("name".to_owned(), InputValue::string("Alex")),
            (
                "page".to_owned(),
                InputValue {
                    text: "3".into(),
                    tag: NbtTag::Float(3.0),
                },
            ),
        ];
        let DialogClick::Custom { id, payload } = action.resolve(&values) else {
            panic!("a custom click");
        };
        assert_eq!(id, "test:submit");
        let Some(NbtTag::Compound(tag)) = payload else {
            panic!("a compound");
        };
        assert_eq!(tag.get("name"), Some(&NbtTag::String("Alex".into())));
        assert_eq!(tag.get("page"), Some(&NbtTag::Float(3.0)));
        assert_eq!(tag.get("flag"), Some(&NbtTag::Byte(1)));
    }

    #[test]
    fn payloads_are_length_prefixed() {
        let mut out = Vec::new();
        write_payload(&mut out, None).unwrap();
        assert_eq!(out, [1, 0]);
        out.clear();
        write_payload(&mut out, Some(&NbtTag::Byte(5))).unwrap();
        assert_eq!(out, [2, 1, 5]);
    }
}
