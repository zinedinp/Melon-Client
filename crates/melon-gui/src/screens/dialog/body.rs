//! `DialogBodyHandlers`: an element for each kind of dialog body.

use super::element::{Element, ItemElement, TextElement};
use crate::dialog::DialogBody;
use crate::screen::ScreenContext;

/// `DialogBodyHandlers.createBodyElement`.
pub fn element(body: &DialogBody, ctx: &ScreenContext) -> Box<dyn Element> {
    match body {
        DialogBody::PlainMessage(message) => Box::new(TextElement::new(
            &message.contents,
            message.width as f32,
            ctx,
        )),
        DialogBody::Item(item) => Box::new(ItemElement::new(item, ctx)),
    }
}
