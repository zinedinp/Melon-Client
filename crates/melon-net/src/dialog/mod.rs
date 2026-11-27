//! Screens a server describes (vanilla `server/dialog`): a title, a body of
//! text and items, inputs, and buttons whose actions are click events, command
//! templates or custom payloads sent back. This is the data; the screens that
//! show it are the GUI's (`gui::screens::dialog`), and what the connection
//! said about dialogs (the registry, the server's links, the requests to show
//! one) is in [`connection`].
//!
//! Each part is a closed set of types here, as vanilla registers them
//! (`DialogTypes`, `DialogBodyTypes`, `InputControlTypes`, `ActionTypes`):
//! a new type is a new variant and its parser, and the GUI picks it up by
//! giving the variant a handler.

pub mod action;
pub mod body;
pub mod connection;
pub mod input;
pub mod links;
mod parse;

use std::sync::Arc;

use crate::component::Component;
pub use action::{Action, DialogClick, InputValue, Template};
pub use body::{DialogBody, ItemBody, PlainMessage};
pub use input::{Input, InputControl};

/// `CommonButtonData.DEFAULT_WIDTH`.
pub const DEFAULT_BUTTON_WIDTH: u32 = 150;
/// `Dialogs.BIG_BUTTON_WIDTH`: the built-in lists' buttons.
pub const BIG_BUTTON_WIDTH: u32 = 310;

/// `DialogAction`: what becomes of the dialog once a button acted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AfterAction {
    /// Back to the screen under the dialog.
    #[default]
    Close,
    /// The dialog stays.
    None,
    /// `WaitingForResponseScreen` until the server shows something else.
    WaitForResponse,
}

impl AfterAction {
    /// `DialogAction.willUnpause`.
    pub fn will_unpause(self) -> bool {
        matches!(self, Self::Close | Self::WaitForResponse)
    }
}

/// `CommonDialogData`: what every kind of dialog has.
#[derive(Clone, Debug)]
pub struct CommonData {
    pub title: Component,
    /// The name of a button that opens this dialog (`computeExternalTitle`).
    pub external_title: Option<Component>,
    pub can_close_with_escape: bool,
    /// Pauses a singleplayer game while open.
    pub pause: bool,
    pub after_action: AfterAction,
    pub body: Vec<DialogBody>,
    pub inputs: Vec<Input>,
}

impl CommonData {
    /// `CommonDialogData.computeExternalTitle`.
    pub fn external_title(&self) -> &Component {
        self.external_title.as_ref().unwrap_or(&self.title)
    }

    /// A dialog with only a title, as the built-in lists are.
    fn titled(title: &str, external_title: &str) -> Self {
        Self {
            title: Component::translatable(title, Vec::new()),
            external_title: Some(Component::translatable(external_title, Vec::new())),
            can_close_with_escape: true,
            pause: true,
            after_action: AfterAction::Close,
            body: Vec::new(),
            inputs: Vec::new(),
        }
    }
}

/// `CommonButtonData`.
#[derive(Clone, Debug)]
pub struct ButtonData {
    pub label: Component,
    pub tooltip: Option<Component>,
    /// 1 to 1024.
    pub width: u32,
}

/// `ActionButton`: a button and what it does, if anything.
#[derive(Clone, Debug)]
pub struct ActionButton {
    pub button: ButtonData,
    pub action: Option<Action>,
}

impl ActionButton {
    /// A button that only closes, labelled with a translation.
    pub fn plain(key: &str, width: u32) -> Self {
        Self {
            button: ButtonData {
                label: Component::translatable(key, Vec::new()),
                tooltip: None,
                width,
            },
            action: None,
        }
    }
}

/// The dialogs a `dialog_list` opens: some, or a tag of the registry's.
#[derive(Clone, Debug)]
pub enum DialogSet {
    Each(Vec<DialogRef>),
    Tag(String),
}

/// `Holder<Dialog>`: one given whole, or a registry entry by name or (as
/// the play phase's packets name it) by id.
#[derive(Clone, Debug)]
pub enum DialogRef {
    Inline(Arc<Dialog>),
    Named(String),
    Id(usize),
}

/// `Dialog`'s kinds (`DialogTypes`).
#[derive(Clone, Debug)]
pub enum DialogKind {
    /// `NoticeDialog`: one button, OK by default.
    Notice { action: ActionButton },
    /// `ConfirmationDialog`: yes and no.
    Confirmation { yes: ActionButton, no: ActionButton },
    /// `MultiActionDialog`: buttons in columns, an optional exit below.
    MultiAction {
        actions: Vec<ActionButton>,
        exit: Option<ActionButton>,
        columns: u32,
    },
    /// `ServerLinksDialog`: a button per link the server sent.
    ServerLinks {
        exit: Option<ActionButton>,
        columns: u32,
        button_width: u32,
    },
    /// `DialogListDialog`: a button per dialog, opening it.
    DialogList {
        dialogs: DialogSet,
        exit: Option<ActionButton>,
        columns: u32,
        button_width: u32,
    },
}

/// `Dialog`.
#[derive(Clone, Debug)]
pub struct Dialog {
    pub common: CommonData,
    pub kind: DialogKind,
}

impl Dialog {
    /// `Dialog.DIRECT_CODEC` over a dialog's JSON (NBT turned to JSON with
    /// bytes as booleans). `None` for one vanilla would refuse.
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        parse::dialog(value)
    }

    /// `Dialog.onCancel`: what Escape (or closing) does.
    pub fn on_cancel(&self) -> Option<&Action> {
        match &self.kind {
            DialogKind::Notice { action } => action.action.as_ref(),
            DialogKind::Confirmation { no, .. } => no.action.as_ref(),
            DialogKind::MultiAction { exit, .. }
            | DialogKind::ServerLinks { exit, .. }
            | DialogKind::DialogList { exit, .. } => exit.as_ref()?.action.as_ref(),
        }
    }

    /// `Dialog.CODEC`: a registry name or a dialog given whole, as a
    /// `show_dialog` click event carries it.
    pub fn reference(value: &serde_json::Value) -> Option<DialogRef> {
        parse::dialog_ref(value)
    }

    /// `Dialogs.SERVER_LINKS`, for a registry that lacks it.
    pub fn builtin_server_links() -> Self {
        Self {
            common: CommonData::titled("menu.server_links.title", "menu.server_links"),
            kind: DialogKind::ServerLinks {
                exit: Some(ActionButton::plain("gui.back", 200)),
                columns: 1,
                button_width: BIG_BUTTON_WIDTH,
            },
        }
    }

    /// `Dialogs.CUSTOM_OPTIONS` and `QUICK_ACTIONS`: the dialogs of a tag in
    /// a list, for a registry that lacks them.
    pub fn builtin_list(title: &str, external_title: &str, tag: &str) -> Self {
        Self {
            common: CommonData::titled(title, external_title),
            kind: DialogKind::DialogList {
                dialogs: DialogSet::Tag(tag.to_owned()),
                exit: Some(ActionButton::plain("gui.back", 200)),
                columns: 1,
                button_width: BIG_BUTTON_WIDTH,
            },
        }
    }
}
