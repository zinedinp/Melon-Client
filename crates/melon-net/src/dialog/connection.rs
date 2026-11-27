//! What the connection says about dialogs (vanilla
//! `ClientCommonPacketListenerImpl`'s part): the `minecraft:dialog` registry
//! and its tags, the server's links, the dialogs it asks to show or clear,
//! and the way back for custom actions.
//!
//! The configuration phase runs on the connection's thread while the menus
//! are up, and the play phase on another, so like the server's resource
//! packs (`assets::packs::server`) this is shared state the connection writes
//! and the game loop reads once a frame ([`poll`]).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::Result;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_protocol::ser::{NetworkReadSliceExt, nbt_tag_to_json_ext};
use pumpkin_util::version::JavaMinecraftVersion;

use super::links::ServerLink;
use super::{Dialog, DialogRef, DialogSet};
use crate::network::protocol::reader::{Reader, initial_capacity};

const VERSION: JavaMinecraftVersion = JavaMinecraftVersion::V_26_3;
const REGISTRY: &str = "minecraft:dialog";
/// `DialogTags.PAUSE_SCREEN_ADDITIONS` / `QUICK_ACTIONS`.
pub const PAUSE_SCREEN_ADDITIONS: &str = "minecraft:pause_screen_additions";
pub const QUICK_ACTIONS: &str = "minecraft:quick_actions";

/// Sends `ServerboundCustomClickActionPacket` (an id and its payload) in
/// whichever phase the connection is.
pub type Sender = Arc<dyn Fn(&str, Option<&NbtTag>) + Send + Sync>;

/// What the server asked the screens to do.
#[derive(Clone, Debug)]
pub enum Request {
    /// `ClientboundShowDialogPacket`.
    Show(Arc<Dialog>),
    /// `ClientboundClearDialogPacket`.
    Clear,
}

#[derive(Default)]
struct State {
    /// The registry's entries in id order; `None` for one that didn't parse.
    registry: Vec<(String, Option<Arc<Dialog>>)>,
    /// Tag name to registry ids.
    tags: HashMap<String, Vec<usize>>,
    links: Vec<ServerLink>,
    requests: VecDeque<Request>,
    sender: Option<Sender>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state() -> MutexGuard<'static, Option<State>> {
    STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    f(state().get_or_insert_with(State::default))
}

/// A new connection: nothing from the last one is kept.
pub fn start(sender: Sender) {
    *state() = Some(State {
        sender: Some(sender),
        ..State::default()
    });
}

/// The connection is gone.
pub fn end() {
    *state() = None;
}

/// The next thing the server asked for, if any.
pub fn poll() -> Option<Request> {
    state().as_mut()?.requests.pop_front()
}

/// `ClientboundRegistryDataPacket`: the dialogs, if it is their registry.
pub fn read_registry(body: &[u8]) -> Result<()> {
    let mut r = Reader::new(body);
    if r.string()? != REGISTRY {
        return Ok(());
    }
    let count = r.list_count()?;
    let mut entries = Vec::with_capacity(initial_capacity(count));
    for _ in 0..count {
        let name = r.string()?;
        let dialog = if r.bool()? {
            let json = read_json(&mut r)?;
            let parsed = json.as_ref().and_then(Dialog::from_json).map(Arc::new);
            if parsed.is_none() {
                crate::warn!("Dialog {name} from the registry unreadable");
            }
            parsed
        } else {
            None
        };
        entries.push((name, dialog));
    }
    with(|state| state.registry = entries);
    Ok(())
}

/// `ClientboundUpdateTagsPacket`: the dialog registry's tags.
pub fn read_tags(body: &[u8]) -> Result<()> {
    let mut r = Reader::new(body);
    let registries = r.list_count()?;
    for _ in 0..registries {
        let registry = r.string()?;
        let count = r.list_count()?;
        let mut tags = HashMap::new();
        for _ in 0..count {
            let name = r.string()?;
            let ids = r.list_count()?;
            let mut entries = Vec::with_capacity(initial_capacity(ids));
            for _ in 0..ids {
                entries.push(r.var_int()?.max(0) as usize);
            }
            tags.insert(name, entries);
        }
        if registry == REGISTRY {
            with(|state| state.tags = tags);
            return Ok(());
        }
    }
    Ok(())
}

/// `ClientboundShowDialogPacket`: in the configuration phase the dialog
/// itself (`CONTEXT_FREE_STREAM_CODEC`), in play a registry id + 1 or 0 and
/// the dialog.
pub fn read_show(body: &[u8], play: bool) -> Result<()> {
    let mut r = Reader::new(body);
    let dialog = if play {
        match r.var_int()? {
            0 => read_dialog(&mut r)?,
            id => resolve(&DialogRef::Id((id - 1).max(0) as usize)),
        }
    } else {
        read_dialog(&mut r)?
    };
    match dialog {
        Some(dialog) => with(|state| state.requests.push_back(Request::Show(dialog))),
        None => crate::warn!("Dialog from the server unreadable, not shown"),
    }
    Ok(())
}

/// `ClientboundClearDialogPacket`.
pub fn clear() {
    with(|state| state.requests.push_back(Request::Clear));
}

/// `ClientboundServerLinksPacket`.
pub fn read_links(body: &[u8]) -> Result<()> {
    let links = super::links::read(body)?;
    with(|state| state.links = links);
    Ok(())
}

fn read_dialog(r: &mut Reader) -> Result<Option<Arc<Dialog>>> {
    Ok(read_json(r)?
        .as_ref()
        .and_then(Dialog::from_json)
        .map(Arc::new))
}

fn read_json(r: &mut Reader) -> Result<Option<serde_json::Value>> {
    let tag = r
        .remaining()
        .get_nbt(&VERSION)
        .map_err(|e| anyhow::anyhow!("dialog NBT: {e}"))?;
    Ok(tag.map(|tag| nbt_tag_to_json_ext(&tag, true)))
}

/// The server's links, as last sent.
pub fn links() -> Vec<ServerLink> {
    state()
        .as_ref()
        .map(|s| s.links.clone())
        .unwrap_or_default()
}

/// A dialog by reference: given whole, or the registry's (a registry name
/// that isn't there falls back to vanilla's built-in one, if it is one).
pub fn resolve(dialog: &DialogRef) -> Option<Arc<Dialog>> {
    match dialog {
        DialogRef::Inline(dialog) => Some(Arc::clone(dialog)),
        DialogRef::Named(name) => {
            let found = state().as_ref().and_then(|s| {
                s.registry
                    .iter()
                    .find(|(entry, _)| entry == name)
                    .and_then(|(_, dialog)| dialog.clone())
            });
            found.or_else(|| builtin(name))
        }
        DialogRef::Id(id) => state()
            .as_ref()?
            .registry
            .get(*id)
            .and_then(|(_, dialog)| dialog.clone()),
    }
}

/// A `dialog_list`'s dialogs, the ones that resolve.
pub fn resolve_set(set: &DialogSet) -> Vec<Arc<Dialog>> {
    match set {
        DialogSet::Each(each) => each.iter().filter_map(resolve).collect(),
        DialogSet::Tag(tag) => tagged(tag),
    }
}

fn tagged(tag: &str) -> Vec<Arc<Dialog>> {
    let guard = state();
    let Some(state) = guard.as_ref() else {
        return Vec::new();
    };
    state
        .tags
        .get(tag)
        .into_iter()
        .flatten()
        .filter_map(|&id| state.registry.get(id).and_then(|(_, d)| d.clone()))
        .collect()
}

/// `Dialogs`' built-in entries, for a server whose registry lacks them.
fn builtin(name: &str) -> Option<Arc<Dialog>> {
    Some(Arc::new(match name {
        "minecraft:server_links" => Dialog::builtin_server_links(),
        "minecraft:custom_options" => Dialog::builtin_list(
            "menu.custom_options.title",
            "menu.custom_options",
            PAUSE_SCREEN_ADDITIONS,
        ),
        "minecraft:quick_actions" => Dialog::builtin_list(
            "menu.quick_actions.title",
            "menu.quick_actions",
            QUICK_ACTIONS,
        ),
        _ => return None,
    }))
}

/// `PauseScreen.getCustomAdditions`: the tag's one dialog, `custom_options`
/// for several, else `server_links` when the server sent links.
pub fn pause_screen_addition() -> Option<Arc<Dialog>> {
    let additions = tagged(PAUSE_SCREEN_ADDITIONS);
    match additions.len() {
        0 if links().is_empty() => None,
        0 => resolve(&DialogRef::Named("minecraft:server_links".into())),
        1 => additions.into_iter().next(),
        _ => resolve(&DialogRef::Named("minecraft:custom_options".into())),
    }
}

/// `Minecraft.getQuickActionsDialog`: the tag's one dialog, or
/// `quick_actions` for several.
pub fn quick_actions() -> Option<Arc<Dialog>> {
    let actions = tagged(QUICK_ACTIONS);
    match actions.len() {
        0 => None,
        1 => actions.into_iter().next(),
        _ => resolve(&DialogRef::Named("minecraft:quick_actions".into())),
    }
}

/// `ServerboundCustomClickActionPacket`.
pub fn send_custom(id: &str, payload: Option<&NbtTag>) {
    let sender = state().as_ref().and_then(|s| s.sender.clone());
    match sender {
        Some(send) => send(id, payload),
        None => crate::warn!("Custom action {id} without a connection"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::protocol::{reader, varint};

    /// The state is shared; the tests that touch it take turns.
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn the_servers_registry_and_tags_resolve() {
        let _turn = SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        start(Arc::new(|_, _| {}));
        let body = crate::network::login::world_registries::tests::registry_packet(REGISTRY);
        read_registry(&body).unwrap();
        let links = resolve(&DialogRef::Named("minecraft:server_links".into())).unwrap();
        assert!(matches!(
            links.kind,
            super::super::DialogKind::ServerLinks { .. }
        ));
        assert!(pause_screen_addition().is_none(), "no tag, no links");

        let index = state()
            .as_ref()
            .unwrap()
            .registry
            .iter()
            .position(|(name, _)| name == "minecraft:quick_actions")
            .unwrap();
        let mut tags = Vec::new();
        varint::write_var_int(&mut tags, 2).unwrap();
        reader::write_string(&mut tags, "minecraft:item").unwrap();
        varint::write_var_int(&mut tags, 0).unwrap();
        reader::write_string(&mut tags, REGISTRY).unwrap();
        varint::write_var_int(&mut tags, 1).unwrap();
        reader::write_string(&mut tags, PAUSE_SCREEN_ADDITIONS).unwrap();
        varint::write_var_int(&mut tags, 1).unwrap();
        varint::write_var_int(&mut tags, index as i32).unwrap();
        read_tags(&tags).unwrap();
        let addition = pause_screen_addition().unwrap();
        assert_eq!(
            addition.common.external_title().plain_text(),
            "menu.quick_actions"
        );

        let mut show = Vec::new();
        varint::write_var_int(&mut show, index as i32 + 1).unwrap();
        read_show(&show, true).unwrap();
        clear();
        assert!(matches!(poll(), Some(Request::Show(_))));
        assert!(matches!(poll(), Some(Request::Clear)));
        assert!(poll().is_none());
        end();
    }

    #[test]
    fn builtins_stand_in_for_a_missing_registry() {
        let _turn = SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        end();
        let custom = resolve(&DialogRef::Named("minecraft:custom_options".into())).unwrap();
        assert!(matches!(
            &custom.kind,
            super::super::DialogKind::DialogList { dialogs: DialogSet::Tag(tag), .. }
                if tag == PAUSE_SCREEN_ADDITIONS
        ));
        assert!(resolve(&DialogRef::Named("test:missing".into())).is_none());
    }
}
