//! Chat on the wire (vanilla `ClientPacketListener`'s chat handlers).

pub mod command_suggestions;
pub mod commands;

use anyhow::{Result, bail};
use pumpkin_protocol::ser::NetworkReadSliceExt;
use pumpkin_util::version::JavaMinecraftVersion;
use serde::Deserialize;
use std::io::Write;

use melon_protocol::component::{self, Component, Style};
use melon_protocol::packet::{self, Reader};
use melon_protocol::varint::write_var_int;

pub mod id {
    use pumpkin_data::packet::{clientbound::play as cb, serverbound::play as sb};

    pub const CB_COMMAND_SUGGESTIONS: i32 = cb::COMMAND_SUGGESTIONS.0;
    pub const CB_DELETE_CHAT: i32 = cb::DELETE_CHAT.0;
    pub const CB_DISGUISED_CHAT: i32 = cb::DISGUISED_CHAT.0;
    pub const CB_PLAYER_CHAT: i32 = cb::PLAYER_CHAT.0;
    pub const CB_SET_ACTION_BAR_TEXT: i32 = cb::SET_ACTION_BAR_TEXT.0;
    pub const CB_SYSTEM_CHAT: i32 = cb::SYSTEM_CHAT.0;
    pub const CB_CLEAR_TITLES: i32 = cb::CLEAR_TITLES.0;
    pub const CB_SET_SUBTITLE_TEXT: i32 = cb::SET_SUBTITLE_TEXT.0;
    pub const CB_SET_TITLE_TEXT: i32 = cb::SET_TITLE_TEXT.0;
    pub const CB_SET_TITLES_ANIMATION: i32 = cb::SET_TITLES_ANIMATION.0;

    pub const SB_CHAT: i32 = sb::CHAT.0;
    pub const SB_CHAT_ACK: i32 = sb::CHAT_ACK.0;
    pub const SB_CHAT_COMMAND: i32 = sb::CHAT_COMMAND.0;
    pub const SB_COMMAND_SUGGESTION: i32 = sb::COMMAND_SUGGESTION.0;
}

/// The protocol version the components are read in.
const VERSION: JavaMinecraftVersion = JavaMinecraftVersion::V_26_3;

/// `ChatType.chat()`: how a player message of this type is laid out, e.g.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ChatDecoration {
    pub translation_key: String,
    #[serde(default)]
    pub parameters: Vec<String>,
    #[serde(default, deserialize_with = "style_of")]
    pub style: Option<Style>,
}

/// A chat type's `style` compound.
fn style_of<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<Style>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.map(|v| component::decode::style_of(&v)))
}

/// The `minecraft:chat_type` registry, by network id.
#[derive(Clone, Debug, Default)]
pub struct ChatTypes(Vec<ChatDecoration>);

impl ChatTypes {
    /// Vanilla's built-in types, for servers that leave the data out.
    fn vanilla(name: &str) -> ChatDecoration {
        let (key, params): (&str, &[&str]) = match name.trim_start_matches("minecraft:") {
            "emote_command" => ("chat.type.emote", &["sender", "content"]),
            "msg_command_incoming" => ("commands.message.display.incoming", &["sender", "content"]),
            "msg_command_outgoing" => ("commands.message.display.outgoing", &["target", "content"]),
            "say_command" => ("chat.type.announcement", &["sender", "content"]),
            "team_msg_command_incoming" => {
                ("chat.type.team.text", &["target", "sender", "content"])
            }
            "team_msg_command_outgoing" => {
                ("chat.type.team.sent", &["target", "sender", "content"])
            }
            _ => ("chat.type.text", &["sender", "content"]),
        };
        ChatDecoration {
            translation_key: key.to_owned(),
            parameters: params.iter().map(|p| (*p).to_owned()).collect(),
            style: None,
        }
    }

    /// `ClientboundRegistryDataPacket`, if it is the chat type registry.
    pub fn from_registry_packet(body: &[u8]) -> Result<Option<Self>> {
        let mut r = Reader::new(body);
        if r.string()? != "minecraft:chat_type" {
            return Ok(None);
        }
        let count = r.list_count()?;
        let mut types = Vec::with_capacity(count.min(256));
        for _ in 0..count {
            let name = r.string()?;
            let decoration = if r.bool()? {
                let tag = r
                    .remaining()
                    .get_nbt(&VERSION)
                    .map_err(|e| anyhow::anyhow!("chat type NBT: {e}"))?;
                tag.and_then(|tag| {
                    let json = pumpkin_protocol::ser::nbt_tag_to_json(&tag);
                    serde_json::from_value::<ChatDecoration>(json.get("chat")?.clone()).ok()
                })
                .unwrap_or_else(|| Self::vanilla(&name))
            } else {
                Self::vanilla(&name)
            };
            types.push(decoration);
        }
        Ok(Some(Self(types)))
    }

    /// A `ChatType.Bound`'s type.
    pub fn get(&self, holder_id: i32) -> std::borrow::Cow<'_, ChatDecoration> {
        usize::try_from(holder_id - 1)
            .ok()
            .and_then(|i| self.0.get(i))
            .map_or_else(
                || std::borrow::Cow::Owned(Self::vanilla("chat")),
                std::borrow::Cow::Borrowed,
            )
    }
}

/// `ChatType.Bound`: the type and the names it fills in.
#[derive(Clone, Debug)]
pub struct BoundChatType {
    pub chat_type: i32,
    pub sender_name: Component,
    pub target_name: Option<Component>,
}

/// A chat packet from the server, decoded.
#[derive(Clone, Debug)]
pub enum ChatEvent {
    /// `ClientboundSystemChatPacket`: `overlay` puts it above the hotbar.
    System { content: Component, overlay: bool },
    /// `ClientboundPlayerChatPacket`: a player's message.
    Player {
        sender: uuid::Uuid,
        signature: Option<Box<[u8; 256]>>,
        message: String,
        unsigned_content: Option<Component>,
        /// `FilterMask.FULLY_FILTERED`.
        fully_filtered: bool,
        bound: BoundChatType,
    },
    /// `ClientboundDisguisedChatPacket`.
    Disguised {
        message: Component,
        bound: BoundChatType,
    },
    /// `ClientboundSetActionBarTextPacket`.
    ActionBar(Component),
    /// `ClientboundSetTitleTextPacket`.
    Title(Component),
    /// `ClientboundSetSubtitleTextPacket`.
    Subtitle(Component),
    /// `ClientboundSetTitlesAnimationPacket`, in ticks.
    TitleTimes {
        fade_in: i32,
        stay: i32,
        fade_out: i32,
    },
    /// `ClientboundClearTitlesPacket`: `reset` also restores the times.
    ClearTitles { reset: bool },
    /// `ClientboundDeleteChatPacket` with a full signature.
    Delete(Box<[u8; 256]>),
    /// `ClientboundCommandSuggestionsPacket`.
    Suggestions(Suggestions),
}

/// Tab completions for the text the client asked about.
#[derive(Clone, Debug, Default)]
pub struct Suggestions {
    pub transaction: i32,
    pub start: usize,
    pub length: usize,
    pub matches: Vec<(String, Option<Component>)>,
}

fn signature(r: &mut Reader<'_>) -> Result<Box<[u8; 256]>> {
    let bytes = r.bytes(256)?;
    Ok(Box::new(
        bytes.try_into().map_err(|_| anyhow::anyhow!("signature"))?,
    ))
}

fn bound_chat_type(r: &mut Reader<'_>) -> Result<BoundChatType> {
    let chat_type = r.var_int()?;
    let sender_name = r.component()?;
    let target_name = if r.bool()? {
        Some(r.component()?)
    } else {
        None
    };
    Ok(BoundChatType {
        chat_type,
        sender_name,
        target_name,
    })
}

/// Decodes `pid` if it is a chat packet.
pub fn decode(pid: i32, body: &[u8]) -> Result<Option<ChatEvent>> {
    let mut r = Reader::new(body);
    Ok(Some(match pid {
        id::CB_SYSTEM_CHAT => ChatEvent::System {
            content: r.component()?,
            overlay: r.bool()?,
        },
        id::CB_SET_ACTION_BAR_TEXT => ChatEvent::ActionBar(r.component()?),
        id::CB_SET_TITLE_TEXT => ChatEvent::Title(r.component()?),
        id::CB_SET_SUBTITLE_TEXT => ChatEvent::Subtitle(r.component()?),
        id::CB_SET_TITLES_ANIMATION => ChatEvent::TitleTimes {
            fade_in: r.i32_be()?,
            stay: r.i32_be()?,
            fade_out: r.i32_be()?,
        },
        id::CB_CLEAR_TITLES => ChatEvent::ClearTitles { reset: r.bool()? },
        id::CB_DISGUISED_CHAT => ChatEvent::Disguised {
            message: r.component()?,
            bound: bound_chat_type(&mut r)?,
        },
        id::CB_PLAYER_CHAT => {
            let _global_index = r.var_int()?;
            let sender = r.uuid()?;
            let _index = r.var_int()?;
            let signature = if r.bool()? {
                Some(signature(&mut r)?)
            } else {
                None
            };
            let message = r.string()?;
            let _timestamp = r.i64_be()?;
            let _salt = r.i64_be()?;
            let previous = r.var_int()?.max(0);
            for _ in 0..previous {
                if r.var_int()? == 0 {
                    r.skip(256)?;
                }
            }
            let unsigned_content = if r.bool()? {
                Some(r.component()?)
            } else {
                None
            };
            let filter = r.var_int()?;
            if filter == 2 {
                let max = r.remaining().len() / 8;
                let longs = r.count(max)?;
                r.skip(longs * 8)?;
            }
            ChatEvent::Player {
                sender,
                signature,
                message,
                unsigned_content,
                fully_filtered: filter == 1,
                bound: bound_chat_type(&mut r)?,
            }
        }
        id::CB_DELETE_CHAT => {
            if r.var_int()? != 0 {
                return Ok(None);
            }
            ChatEvent::Delete(signature(&mut r)?)
        }
        id::CB_COMMAND_SUGGESTIONS => {
            let transaction = r.var_int()?;
            let start = r.var_int()?.max(0) as usize;
            let length = r.var_int()?.max(0) as usize;
            let count = r.list_count()?;
            let mut matches = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                let text = r.string()?;
                let tooltip = if r.bool()? {
                    Some(r.component()?)
                } else {
                    None
                };
                matches.push((text, tooltip));
            }
            ChatEvent::Suggestions(Suggestions {
                transaction,
                start,
                length,
                matches,
            })
        }
        _ => return Ok(None),
    }))
}

/// `ServerboundChatPacket`, unsigned.
pub fn write_chat(buf: &mut Vec<u8>, message: &str, offset: i32) -> Result<()> {
    if message.encode_utf16().count() > 256 {
        bail!("chat message longer than 256");
    }
    packet::write_string(buf, message)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    buf.write_all(&now.to_be_bytes())?;
    buf.write_all(&rand::random::<i64>().to_be_bytes())?;
    packet::write_bool(buf, false)?;
    write_var_int(buf, offset)?;
    buf.write_all(&[0; 3])?; // `LastSeenMessages.Update`'s 20-bit set
    buf.write_all(&[0])?;
    Ok(())
}

/// `ServerboundChatCommandPacket`: a command without the slash.
pub fn write_command(buf: &mut Vec<u8>, command: &str) -> Result<()> {
    packet::write_string(buf, command)
}

/// `ServerboundChatAckPacket`.
pub fn write_ack(buf: &mut Vec<u8>, offset: i32) -> Result<()> {
    Ok(write_var_int(buf, offset)?)
}

/// `ServerboundCommandSuggestionPacket`.
pub fn write_suggestion_request(buf: &mut Vec<u8>, transaction: i32, text: &str) -> Result<()> {
    write_var_int(buf, transaction)?;
    packet::write_string(buf, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A compound of string entries as the network writes it (no root name).
    fn compound(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut out = vec![0x0A];
        for (name, value) in entries {
            out.push(8);
            out.extend((name.len() as u16).to_be_bytes());
            out.extend(name.as_bytes());
            out.extend((value.len() as u16).to_be_bytes());
            out.extend(value.as_bytes());
        }
        out.push(0);
        out
    }

    #[test]
    fn reads_a_system_chat_with_its_colour() {
        let mut body = compound(&[("text", "Hallo"), ("color", "yellow")]);
        body.push(1);
        let Some(ChatEvent::System { content, overlay }) =
            decode(id::CB_SYSTEM_CHAT, &body).unwrap()
        else {
            panic!("not a system message");
        };
        assert!(overlay);
        assert_eq!(content.plain_text(), "Hallo");
        assert_eq!(content.style.color, Some(component::TextColor::YELLOW));
    }

    #[test]
    fn compact_components_are_expanded() {
        let json = serde_json::json!({
            "color": "yellow",
            "translate": "multiplayer.player.joined",
            "with": ["Chatter", 3]
        });
        let component = component::decode::component(&json);
        let lang = melon_assets::i18n::Lang::from_pairs(&[("multiplayer.player.joined", "%s joined %s")]);
        let text = melon_assets::text::StyledText::from_component(&component, &lang);
        assert_eq!(text.to_plain(), "Chatter joined 3");
        let list = component::decode::component(&serde_json::json!(["a", {"text": "b"}]));
        assert_eq!(
            melon_assets::text::StyledText::from_component(&list, &lang).to_plain(),
            "ab"
        );
    }

    #[test]
    fn byte_flags_keep_the_colours() {
        let mut bold = pumpkin_nbt::compound::NbtCompound::new();
        bold.put_string("text", "Coins".to_owned());
        bold.put("bold", pumpkin_nbt::tag::NbtTag::Byte(1));
        bold.put_string("color", "gold".to_owned());
        let mut line = pumpkin_nbt::compound::NbtCompound::new();
        line.put_string("text", String::new());
        line.put("italic", pumpkin_nbt::tag::NbtTag::Byte(0));
        line.put(
            "extra",
            pumpkin_nbt::tag::NbtTag::List(vec![
                pumpkin_nbt::tag::NbtTag::Compound(bold),
                pumpkin_nbt::tag::NbtTag::String(": 5".into()),
            ]),
        );
        let json =
            pumpkin_protocol::ser::nbt_tag_to_json(&pumpkin_nbt::tag::NbtTag::Compound(line));
        let component = component::decode::component(&json);
        let text = melon_assets::text::StyledText::from_component(&component, &melon_assets::i18n::Lang::empty());
        assert_eq!(text.to_plain(), "Coins: 5");
        assert!(text.spans[0].style.bold);
        assert_eq!(text.spans[0].style.color, Some(melon_assets::text::colors::GOLD));
    }

    #[test]
    fn chat_type_holders_count_from_one() {
        let types = ChatTypes(vec![
            ChatTypes::vanilla("chat"),
            ChatTypes::vanilla("say_command"),
        ]);
        assert_eq!(types.get(2).translation_key, "chat.type.announcement");
        assert_eq!(types.get(99).translation_key, "chat.type.text");
    }
}
