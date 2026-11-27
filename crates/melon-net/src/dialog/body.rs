//! What a dialog shows above its inputs (vanilla `server/dialog/body`).

use melon_protocol::component::Component;

/// `PlainMessage.DEFAULT_WIDTH`.
pub const PLAIN_MESSAGE_WIDTH: u32 = 200;

/// `DialogBody`'s kinds (`DialogBodyTypes`).
#[derive(Clone, Debug)]
pub enum DialogBody {
    PlainMessage(PlainMessage),
    Item(ItemBody),
}

/// `PlainMessage`: text wrapped to `width`, centred.
#[derive(Clone, Debug)]
pub struct PlainMessage {
    pub contents: Component,
    pub width: u32,
}

/// `ItemBody`: an item, and text beside it.
#[derive(Clone, Debug)]
pub struct ItemBody {
    /// `ItemStackTemplate`: the item's id, count and component patch.
    pub id: String,
    pub count: i32,
    pub components: Option<serde_json::Value>,
    pub description: Option<PlainMessage>,
    /// The count and the durability bar.
    pub show_decorations: bool,
    pub show_tooltip: bool,
    /// The widget's size, 1 to 256 each; the item is drawn at its top left.
    pub width: u32,
    pub height: u32,
}
