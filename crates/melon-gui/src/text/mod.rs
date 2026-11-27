//! Styled text: the model is melon-assets' [`melon_assets::text`]; here are
//! the fonts that draw it and its layout.

pub mod font;
mod layout;
pub mod objects;
pub mod splash;

pub use layout::{TextPaint, draw, is_active, style_at, width, wrap};
pub use melon_assets::text::*;
