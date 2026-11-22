//! Enchantment glint (`ItemStack.hasFoil`, `TextureTransform.setupGlintTexturing`).

use glam::Vec2;
use pumpkin_data::data_component_impl::{
    EnchantmentGlintOverrideImpl, EnchantmentsImpl, LodestoneTrackerImpl,
};
use pumpkin_data::item_stack::ItemStack;

/// `TextureTransform.GLINT_TEXTURING`: items in the GUI and in first person.
pub const ITEM_SCALE: f32 = 8.0;
/// `TextureTransform.ENTITY_GLINT_TEXTURING`: items held in the world.
pub const ENTITY_SCALE: f32 = 0.5;
/// `TextureTransform.ARMOR_ENTITY_GLINT_TEXTURING`: worn armor.
pub const ARMOR_SCALE: f32 = 0.16;

/// A 16px sprite on a 1024 atlas, the usual vanilla `items.png` packing.
const SPRITE_ATLAS_SPAN: f32 = 16.0 / 1024.0;
/// [`ITEM_SCALE`] on sprite-local UVs: 8 × 16/1024.
pub const ITEM_SPRITE_SCALE: f32 = ITEM_SCALE * SPRITE_ATLAS_SPAN;
/// [`ENTITY_SCALE`] on sprite-local UVs.
pub const ENTITY_SPRITE_SCALE: f32 = ENTITY_SCALE * SPRITE_ATLAS_SPAN;

/// `ItemStack.hasFoil`.
pub fn has_foil(stack: &ItemStack) -> bool {
    if stack.is_empty() {
        return false;
    }
    if let Some(over) = stack.get_data_component::<EnchantmentGlintOverrideImpl>() {
        return over.glint;
    }
    if stack.get_data_component::<LodestoneTrackerImpl>().is_some() {
        return true;
    }
    stack
        .get_data_component::<EnchantmentsImpl>()
        .is_some_and(|e| !e.enchantment.is_empty())
}

/// `Util.getMillis() * glintSpeed * 8` from process start, speed 1.
pub fn millis() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    (START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        * 1000.0
        * 8.0) as f32
}

/// `TextureTransform.setupGlintTexturing` on a model's UV.
pub fn uv(uv: Vec2, scale: f32, millis: f32) -> Vec2 {
    GlintUv::new(scale, millis).at(uv)
}

/// [`uv`] worked out once for many texels of one moment.
#[derive(Clone, Copy, Debug)]
pub struct GlintUv {
    sin: f32,
    cos: f32,
    scale: f32,
    offset: Vec2,
}

impl GlintUv {
    pub fn new(scale: f32, millis: f32) -> Self {
        let o0 = millis.rem_euclid(110_000.0) / 110_000.0;
        let o1 = millis.rem_euclid(30_000.0) / 30_000.0;
        let (sin, cos) = (std::f32::consts::PI / 18.0).sin_cos();
        Self {
            sin,
            cos,
            scale,
            offset: Vec2::new(-o0, o1),
        }
    }

    pub fn at(&self, uv: Vec2) -> Vec2 {
        let p = uv * self.scale;
        Vec2::new(
            p.x * self.cos - p.y * self.sin,
            p.x * self.sin + p.y * self.cos,
        ) + self.offset
    }
}

/// A wrapping sample of `texture` at `uv`.
pub fn sample(texture: &image::RgbaImage, uv: Vec2) -> image::Rgba<u8> {
    let w = texture.width() as i32;
    let h = texture.height() as i32;
    if w <= 0 || h <= 0 {
        return image::Rgba([0; 4]);
    }
    let x = uv.x.rem_euclid(1.0) * w as f32;
    let y = uv.y.rem_euclid(1.0) * h as f32;
    let tx = (x as i32).rem_euclid(w) as u32;
    let ty = (y as i32).rem_euclid(h) as u32;
    *texture.get_pixel(tx, ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::item::Item;

    #[test]
    fn an_enchanted_book_always_shimmers() {
        assert!(has_foil(&ItemStack::new(1, &Item::ENCHANTED_BOOK)));
        assert!(has_foil(&ItemStack::new(1, &Item::ENCHANTED_GOLDEN_APPLE)));
        assert!(!has_foil(&ItemStack::new(1, &Item::DIAMOND_SWORD)));
        assert!(!has_foil(ItemStack::EMPTY));
    }

    #[test]
    fn sprite_scale_matches_a_16px_item_on_a_1024_atlas() {
        assert_eq!(ITEM_SPRITE_SCALE, 0.125);
        assert_eq!(ENTITY_SPRITE_SCALE, 0.5 * 16.0 / 1024.0);
    }

    #[test]
    fn worn_enchantments_turn_the_glint_on() {
        let mut stack = ItemStack::new(1, &Item::DIAMOND_SWORD);
        stack.set_data_component(EnchantmentsImpl {
            enchantment: std::borrow::Cow::Owned(vec![(
                pumpkin_data::Enchantment::from_name("minecraft:sharpness").unwrap(),
                5,
            )]),
        });
        assert!(has_foil(&stack));
    }
}
