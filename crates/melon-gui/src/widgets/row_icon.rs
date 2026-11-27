//! The 32x32 icon at the left of a list row with vanilla's overlay arrows.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use image::RgbaImage;
use rustc_hash::FxHashMap;

use melon_render::TextureId;
use crate::sprite::GuiSprite;
use crate::widgets::{Rect, Ui};

/// `SelectableEntry`'s `size`.
pub const SIZE: f32 = 32.0;
/// `-1601138544`: the veil over the hovered row's icon.
const VEIL: [f32; 4] = [144.0 / 255.0, 144.0 / 255.0, 144.0 / 255.0, 160.0 / 255.0];

/// An arrow's two looks.
#[derive(Clone, Copy)]
pub struct Arrow {
    pub normal: GuiSprite,
    pub highlighted: GuiSprite,
}

/// One list's arrow sprites.
pub struct RowIconSprites {
    /// The right half: join a server or world, select a pack.
    pub primary: Arrow,
    /// The left quarters, for lists whose rows move.
    pub up: Option<Arrow>,
    pub down: Option<Arrow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Primary,
    MoveUp,
    MoveDown,
}

/// Where a list puts its arrows on the icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// The whole icon is the one action.
    Whole,
    /// Join on the right half, move up and down on the left quarters.
    Server,
    /// Unselect on the left half, move up and down on the right quarters.
    Selected,
}

/// Which neighbours a row can swap with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moves {
    pub up: bool,
    pub down: bool,
}

impl Moves {
    pub const NONE: Self = Self {
        up: false,
        down: false,
    };

    /// Row `index` of `len`, in a list whose rows move.
    pub fn of(index: usize, len: usize) -> Self {
        Self {
            up: index > 0,
            down: index + 1 < len,
        }
    }
}

/// The zone under the mouse at `(x, y)` for an icon at `icon`'s corner.
pub fn zone(icon: Rect, (x, y): (f32, f32), layout: Layout, moves: Moves) -> Option<Zone> {
    let (rel_x, rel_y) = ((x - icon.x).floor(), (y - icon.y).floor());
    let half = SIZE / 2.0;
    if !(0.0..SIZE).contains(&rel_x) || !(0.0..SIZE).contains(&rel_y) {
        return None;
    }
    let (primary_side, move_side) = match layout {
        Layout::Whole => return Some(Zone::Primary),
        Layout::Server => (rel_x >= half, rel_x < half),
        Layout::Selected => (rel_x < half, rel_x >= half),
    };
    if primary_side {
        Some(Zone::Primary)
    } else if !move_side {
        None
    } else if rel_y < half && moves.up {
        Some(Zone::MoveUp)
    } else if rel_y >= half && moves.down {
        Some(Zone::MoveDown)
    } else {
        None
    }
}

/// The veil alone, for a hovered row that has no arrows (a required pack).
pub fn veil(ui: &mut Ui, icon: Rect) {
    ui.fill(icon, VEIL);
}

/// The veil and arrows over a hovered row's icon, `zone` highlighted.
pub fn draw(ui: &mut Ui, icon: Rect, sprites: &RowIconSprites, zone: Option<Zone>, moves: Moves) {
    veil(ui, icon);
    let pick = |arrow: Arrow, this: Zone| {
        if zone == Some(this) {
            arrow.highlighted
        } else {
            arrow.normal
        }
    };
    ui.sprite(pick(sprites.primary, Zone::Primary), icon);
    if let Some(arrow) = sprites.up.filter(|_| moves.up) {
        ui.sprite(pick(arrow, Zone::MoveUp), icon);
    }
    if let Some(arrow) = sprites.down.filter(|_| moves.down) {
        ui.sprite(pick(arrow, Zone::MoveDown), icon);
    }
}

/// Textures already on the GPU, by content.
fn uploaded() -> &'static Mutex<FxHashMap<u64, TextureId>> {
    static ICONS: OnceLock<Mutex<FxHashMap<u64, TextureId>>> = OnceLock::new();
    ICONS.get_or_init(Default::default)
}

/// `image` as a texture, uploaded through `upload` unless the same pixels are on the GPU
/// already.
pub fn upload_cached(
    image: &RgbaImage,
    upload: &mut dyn FnMut(&RgbaImage) -> Option<TextureId>,
) -> Option<TextureId> {
    let mut hasher = DefaultHasher::new();
    (image.width(), image.height()).hash(&mut hasher);
    image.as_raw().hash(&mut hasher);
    let key = hasher.finish();
    let mut cache = uploaded().lock().ok()?;
    if let Some(texture) = cache.get(&key) {
        return Some(*texture);
    }
    let texture = upload(image)?;
    cache.insert(key, texture);
    Some(texture)
}

/// `FaviconTexture.upload`.
pub fn is_favicon(image: &RgbaImage) -> bool {
    image.width() == FAVICON_SIZE && image.height() == FAVICON_SIZE
}

const FAVICON_SIZE: u32 = 64;

#[cfg(test)]
mod tests {
    use super::*;

    const ICON: Rect = Rect::new(10.0, 20.0, SIZE, SIZE);

    #[test]
    fn the_server_icon_splits_into_join_and_two_move_quarters() {
        let all = Moves::of(1, 3);
        let zone = |p| zone(ICON, p, Layout::Server, all);
        assert_eq!(zone((30.0, 25.0)), Some(Zone::Primary));
        assert_eq!(zone((30.0, 50.0)), Some(Zone::Primary));
        assert_eq!(zone((12.0, 22.0)), Some(Zone::MoveUp));
        assert_eq!(zone((12.0, 45.0)), Some(Zone::MoveDown));
        assert_eq!(zone((9.0, 22.0)), None);
        assert_eq!(zone((42.0, 22.0)), None);
    }

    #[test]
    fn the_selected_pack_icon_mirrors_it() {
        let all = Moves::of(1, 3);
        let zone = |p| zone(ICON, p, Layout::Selected, all);
        assert_eq!(zone((12.0, 22.0)), Some(Zone::Primary));
        assert_eq!(zone((12.0, 50.0)), Some(Zone::Primary));
        assert_eq!(zone((30.0, 25.0)), Some(Zone::MoveUp));
        assert_eq!(zone((30.0, 45.0)), Some(Zone::MoveDown));
        assert_eq!(zone((42.0, 22.0)), None);
    }

    #[test]
    fn a_world_or_available_pack_icon_is_one_button() {
        for p in [(11.0, 21.0), (41.0, 51.0), (12.0, 50.0), (40.0, 22.0)] {
            assert_eq!(
                zone(ICON, p, Layout::Whole, Moves::NONE),
                Some(Zone::Primary)
            );
        }
        assert_eq!(zone(ICON, (42.0, 22.0), Layout::Whole, Moves::NONE), None);
    }

    #[test]
    fn the_ends_of_a_list_have_one_arrow_less() {
        let s = Layout::Server;
        assert_eq!(zone(ICON, (12.0, 22.0), s, Moves::of(0, 3)), None);
        assert_eq!(zone(ICON, (12.0, 45.0), s, Moves::of(2, 3)), None);
        assert_eq!(zone(ICON, (12.0, 22.0), s, Moves::NONE), None);
    }
}
