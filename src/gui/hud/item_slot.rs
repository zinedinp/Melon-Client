//! Items in the GUI (`GuiGraphicsExtractor.item` + `itemDecorations`).

use std::sync::atomic::{AtomicBool, Ordering};

use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::{
    DamageImpl, EnchantmentsImpl, InstrumentImpl, ItemNameImpl, LoreImpl, MaxDamageImpl,
    PaintingVariantImpl, PotionContentsImpl, ProfileImpl, Rarity, StoredEnchantmentsImpl,
};
use pumpkin_data::effect::MobEffectCategory;
use pumpkin_data::item_stack::ItemStack;

use crate::gui::hud::item_icon::ItemIcons;
use crate::gui::text::font::Font;
use crate::gui::text::{self, StyledText, TextPaint, TextStyle, colors};
use crate::gui::widgets::tooltip;
use crate::i18n::Lang;
use crate::math::argb;
use crate::renderer::TextureId;
use crate::renderer::quad::{DrawList, Program};

/// Everything drawing an item needs, bundled so the HUD and the screens pass one value.
#[derive(Clone, Copy)]
pub struct ItemPainter<'a> {
    pub icons: &'a ItemIcons,
    pub font: &'a Font,
    pub font_textures: &'a [TextureId],
    pub white: TextureId,
    pub tooltip: Option<crate::gui::assets::TooltipSprites>,
}

impl ItemPainter<'_> {
    /// Draws `stack` with its top-left corner at `(x, y)`, 16x16.
    pub fn stack(&self, draw_list: &mut DrawList, stack: &ItemStack, x: f32, y: f32) {
        self.stack_with_count(draw_list, stack, x, y, None);
    }

    /// The item alone, without its count or durability bar.
    pub fn icon(&self, draw_list: &mut DrawList, stack: &ItemStack, x: f32, y: f32) {
        if stack.is_empty() {
            return;
        }
        if let Some(icon) = self.icons.get(stack) {
            let previous =
                crate::world::glint::has_foil(stack).then(|| draw_list.set_program(Program::Glint));
            icon.draw(draw_list, x, y, 16.0, 16.0);
            if let Some(previous) = previous {
                draw_list.set_program(previous);
            }
        }
    }

    /// [`Self::stack`] with the count text replaced (`itemDecorations`'
    pub fn stack_with_count(
        &self,
        draw_list: &mut DrawList,
        stack: &ItemStack,
        x: f32,
        y: f32,
        count: Option<StyledText>,
    ) {
        if stack.is_empty() {
            return;
        }
        self.icon(draw_list, stack, x, y);

        if let Some(max) = stack.get_max_damage().filter(|&max| max > 0) {
            let damage = stack.get_damage();
            if damage > 0 {
                let fraction = ((max - damage) as f32 / max as f32).clamp(0.0, 1.0);
                let width = (13.0 - damage as f32 * 13.0 / max as f32).round().max(0.0);
                let [r, g, b] = hsv_to_rgb(fraction / 3.0);
                draw_list.push_fill(self.white, x + 2.0, y + 13.0, 13.0, 2.0, argb(0xFF00_0000));
                draw_list.push_fill(self.white, x + 2.0, y + 13.0, width, 1.0, [r, g, b, 1.0]);
            }
        }

        let count = count.or_else(|| {
            (stack.item_count != 1).then(|| StyledText::plain(&stack.item_count.to_string()))
        });
        if let Some(count) = count {
            let width = text::width(self.font, &count);
            text::draw(
                draw_list,
                &self.paint(colors::WHITE),
                &count,
                x + 19.0 - 2.0 - width,
                y + 6.0 + 3.0,
            );
        }
    }

    pub fn paint(&self, color: [f32; 3]) -> TextPaint<'_> {
        TextPaint {
            font: self.font,
            pages: self.font_textures,
            white: self.white,
            color,
            alpha: 1.0,
            shadow: true,
            obfuscation_seed: 0,
        }
    }

    /// A one-line tooltip.
    pub fn tooltip(
        &self,
        draw_list: &mut DrawList,
        text: &str,
        mouse: (f32, f32),
        size: (f32, f32),
    ) {
        self.tooltip_lines(draw_list, &[StyledText::plain(text)], mouse, size);
    }

    /// [`tooltip::draw`] beside the mouse, kept on the `size` screen.
    pub fn tooltip_lines(
        &self,
        draw_list: &mut DrawList,
        lines: &[StyledText],
        mouse: (f32, f32),
        size: (f32, f32),
    ) {
        let paint = self.paint(colors::WHITE);
        let at = tooltip::beside_mouse(mouse, tooltip::size(&paint, lines), size);
        tooltip::draw(draw_list, self.tooltip.as_ref(), &paint, lines, at);
    }
}

/// `Mth.hsvToRgb(hue, 1, 1)`.
fn hsv_to_rgb(hue: f32) -> [f32; 3] {
    let h = (hue * 6.0).rem_euclid(6.0);
    let f = h - h.floor();
    match h as i32 {
        0 => [1.0, f, 0.0],
        1 => [1.0 - f, 1.0, 0.0],
        2 => [0.0, 1.0, f],
        3 => [0.0, 1.0 - f, 1.0],
        4 => [f, 0.0, 1.0],
        _ => [1.0, 0.0, 1.0 - f],
    }
}

/// `ItemStack.getItemName` (`Item.getName`).
pub fn display_name(lang: &Lang, stack: &ItemStack) -> String {
    let key = stack.item.registry_key;
    if let Some(name) = potion_name(lang, stack) {
        return name;
    }
    if key == "player_head"
        && let Some(name) = stack
            .get_data_component::<ProfileImpl>()
            .and_then(|p| p.name.as_deref())
    {
        return lang.format("block.minecraft.player_head.named", &[name]);
    }
    if stack
        .patch
        .iter()
        .any(|(id, c)| *id == DataComponent::ItemName && c.is_some())
        && let Some(name) = stack.get_data_component::<ItemNameImpl>()
    {
        return lang
            .raw_dynamic(&name.name)
            .unwrap_or(&name.name)
            .to_owned();
    }
    let (namespace, path) = key.split_once(':').unwrap_or(("minecraft", key));
    let path = path.replace('/', ".");
    let item_key = format!("item.{namespace}.{path}");
    if lang.has(&item_key) {
        return lang.get_dynamic(&item_key).to_owned();
    }
    lang.get_dynamic(&format!("block.{namespace}.{path}"))
        .to_owned()
}

/// `ItemStack.getHoverName`: the custom name, else the item's.
pub fn hover_name(lang: &Lang, stack: &ItemStack) -> String {
    match stack.get_custom_name() {
        Some(custom) => {
            StyledText::from_component(&crate::component::pumpkin::from_pumpkin(custom), lang)
                .to_plain()
        }
        None => display_name(lang, stack),
    }
}

/// `ItemStack.getStyledHoverName`.
pub fn styled_hover_name(lang: &Lang, stack: &ItemStack) -> StyledText {
    let mut style = TextStyle::colored(rarity_color(stack.get_rarity()));
    let mut out = StyledText::default();
    match stack.get_custom_name() {
        Some(custom) => {
            style.italic = true;
            out.push_component(
                &crate::component::pumpkin::from_pumpkin(custom),
                &style,
                lang,
            );
        }
        None => out.push_formatted(&display_name(lang, stack), &style),
    }
    out
}

/// `PotionItem.getName`: `item.minecraft.potion.effect.<potion>`.
fn potion_name(lang: &Lang, stack: &ItemStack) -> Option<String> {
    let contents = stack.get_data_component::<PotionContentsImpl>()?;
    let potion = contents
        .potion_id
        .and_then(|id| u8::try_from(id).ok())
        .and_then(pumpkin_data::potion::Potion::from_id)?;
    let key = format!(
        "item.minecraft.{}.effect.{}",
        stack.item.registry_key, potion.name
    );
    lang.has(&key).then(|| lang.get_dynamic(&key).to_owned())
}

/// `Rarity.color`.
fn rarity_color(rarity: Rarity) -> [f32; 3] {
    match rarity {
        Rarity::Common => colors::WHITE,
        Rarity::Uncommon => colors::YELLOW,
        Rarity::Rare => colors::AQUA,
        Rarity::Epic => [1.0, 85.0 / 255.0, 1.0],
    }
}

/// `enchantment.level.<n>`, or the number past ten.
fn level_name(lang: &Lang, level: i32) -> String {
    let key = format!("enchantment.level.{level}");
    if lang.has(&key) {
        lang.get_dynamic(&key).to_owned()
    } else {
        level.to_string()
    }
}

/// `Enchantment.getFullname`.
pub fn enchantment_name(
    lang: &Lang,
    enchantment: &pumpkin_data::Enchantment,
    level: i32,
) -> String {
    let text = lang.get_dynamic(enchantment.description).to_owned();
    if level != 1 || enchantment.max_level != 1 {
        format!("{text} {}", level_name(lang, level))
    } else {
        text
    }
}

/// `Options.advancedItemTooltips` (F3+H).
static ADVANCED_TOOLTIPS: AtomicBool = AtomicBool::new(false);

pub fn set_advanced_tooltips(on: bool) {
    ADVANCED_TOOLTIPS.store(on, Ordering::Relaxed);
}

pub fn advanced_tooltips() -> bool {
    ADVANCED_TOOLTIPS.load(Ordering::Relaxed)
}

/// `ItemStack.getTooltipLines`; advanced (F3+H) adds the durability and the item id.
pub fn tooltip_lines(lang: &Lang, stack: &ItemStack) -> Vec<StyledText> {
    let mut lines = vec![styled_hover_name(lang, stack)];
    let mut enchantments: Vec<(&'static pumpkin_data::Enchantment, i32)> = Vec::new();
    if let Some(e) = stack.get_data_component::<EnchantmentsImpl>() {
        enchantments.extend(e.enchantment.iter().copied());
    }
    if let Some(e) = stack.get_data_component::<StoredEnchantmentsImpl>() {
        enchantments.extend(e.enchantment.iter().copied());
    }
    for (enchantment, level) in enchantments {
        let curse = enchantment.registry_key.contains("curse");
        let text = enchantment_name(lang, enchantment, level);
        lines.push(StyledText::colored(
            &text,
            if curse { colors::RED } else { colors::GRAY },
        ));
    }
    if let Some(contents) = stack.get_data_component::<PotionContentsImpl>() {
        let potion = contents
            .potion_id
            .and_then(|id| u8::try_from(id).ok())
            .and_then(pumpkin_data::potion::Potion::from_id);
        let effects = potion.map_or(&[][..], |p| p.effects);
        if effects.is_empty() {
            lines.push(StyledText::colored(lang.get("effect.none"), colors::GRAY));
        }
        let scale = match stack.item.registry_key {
            "lingering_potion" => 0.25,
            "tipped_arrow" => 0.125,
            _ => 1.0,
        };
        for effect in effects {
            let mut text = lang
                .get_dynamic(effect.effect_type.translation_key)
                .to_owned();
            if effect.amplifier > 0 {
                let potency_key = format!("potion.potency.{}", effect.amplifier);
                let potency = lang.get_dynamic(&potency_key);
                text = lang.format("potion.withAmplifier", &[&text, potency]);
            }
            let ticks = (effect.duration as f32 * scale) as i32;
            if ticks > 20 {
                let seconds = ticks / 20;
                let duration = format!("{}:{:02}", seconds / 60, seconds % 60);
                text = lang.format("potion.withDuration", &[&text, &duration]);
            }
            let color = match effect.effect_type.category {
                MobEffectCategory::Harmful => colors::RED,
                _ => colors::BLUE,
            };
            lines.push(StyledText::colored(&text, color));
        }
    }
    if let Some(instrument) = stack.get_data_component::<InstrumentImpl>() {
        let id = instrument.instrument.trim_start_matches("minecraft:");
        lines.push(StyledText::colored(
            lang.get_dynamic(&format!("instrument.minecraft.{id}")),
            colors::GRAY,
        ));
    }
    if let Some(lore) = stack.get_data_component::<LoreImpl>() {
        let mut style = TextStyle::colored(colors::DARK_PURPLE);
        style.italic = true;
        for line in &lore.lines {
            let mut text = StyledText::default();
            text.push_component(&crate::component::pumpkin::from_pumpkin(line), &style, lang);
            lines.push(text);
        }
    }
    if let Some(painting) = stack.get_data_component::<PaintingVariantImpl>() {
        let id = painting.value.trim_start_matches("minecraft:");
        for (suffix, color) in [("title", colors::YELLOW), ("author", colors::GRAY)] {
            let key = format!("painting.minecraft.{id}.{suffix}");
            if lang.has(&key) {
                lines.push(StyledText::colored(lang.get_dynamic(&key), color));
            }
        }
    }
    attribute_lines(lang, stack, &mut lines);
    if advanced_tooltips() {
        if let Some(max) = stack.get_data_component::<MaxDamageImpl>()
            && max.max_damage > 0
        {
            let damage = stack
                .get_data_component::<DamageImpl>()
                .map_or(0, |d| d.damage);
            lines.push(StyledText::colored(
                &lang.format(
                    "item.durability",
                    &[
                        &(max.max_damage - damage).to_string(),
                        &max.max_damage.to_string(),
                    ],
                ),
                colors::WHITE,
            ));
        }
        lines.push(StyledText::colored(
            &format!("minecraft:{}", stack.item.registry_key),
            colors::DARK_GRAY,
        ));
    }
    lines
}

/// `HoverEvent.ShowItem`'s stack.
pub fn hover_stack(
    id: &str,
    count: i32,
    components: Option<&serde_json::Value>,
) -> Option<ItemStack> {
    use pumpkin_data::data_component_impl::DataComponentImpl;
    let item = pumpkin_data::item::Item::from_registry_key(id)?;
    let mut stack = ItemStack::new(count.clamp(1, 99) as u8, item);
    if let Some(serde_json::Value::Object(patch)) = components {
        for (name, value) in patch {
            let Some(component) = DataComponent::try_from_name(name) else {
                continue;
            };
            let data = match component {
                DataComponent::CustomName => Some(
                    pumpkin_data::data_component_impl::CustomNameImpl {
                        name: crate::component::pumpkin::to_pumpkin(&hover_component(value)),
                    }
                    .to_dyn(),
                ),
                DataComponent::Lore => value.as_array().map(|lines| {
                    LoreImpl {
                        lines: lines
                            .iter()
                            .map(|line| {
                                crate::component::pumpkin::to_pumpkin(&hover_component(line))
                            })
                            .collect(),
                    }
                    .to_dyn()
                }),
                _ => pumpkin_data::data_component_impl::read_data(component, &component_nbt(value)),
            };
            if let Some(data) = data {
                stack.patch.retain(|(c, _)| *c != component);
                stack.patch.push((component, Some(data)));
            }
        }
    }
    Some(stack)
}

/// A text component in a hover's component patch, in any of its forms.
fn hover_component(value: &serde_json::Value) -> crate::component::Component {
    crate::component::decode::component(value)
}

/// A component's JSON as NBT.
fn component_nbt(value: &serde_json::Value) -> pumpkin_nbt::tag::NbtTag {
    use pumpkin_nbt::tag::NbtTag;
    match value {
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => i32::try_from(i).map_or(NbtTag::Long(i), NbtTag::Int),
            None => NbtTag::Double(n.as_f64().unwrap_or(0.0)),
        },
        serde_json::Value::Array(list) => NbtTag::List(list.iter().map(component_nbt).collect()),
        serde_json::Value::Object(map) => {
            let mut compound = pumpkin_nbt::compound::NbtCompound::new();
            for (key, value) in map {
                compound.put(key, component_nbt(value));
            }
            NbtTag::Compound(compound)
        }
        other => pumpkin_protocol::ser::json_to_nbt_tag(other),
    }
}

/// `ItemStack.addAttributeTooltips` with `Display.Default`.
fn attribute_lines(lang: &Lang, stack: &ItemStack, lines: &mut Vec<StyledText>) {
    use pumpkin_data::AttributeModifierSlot as Slot;
    use pumpkin_data::attributes::Attributes;
    use pumpkin_data::data_component_impl::{AttributeModifiersImpl, Operation};

    let Some(modifiers) = stack.get_data_component::<AttributeModifiersImpl>() else {
        return;
    };
    const SLOTS: [(Slot, &str); 11] = [
        (Slot::Any, "any"),
        (Slot::MainHand, "mainhand"),
        (Slot::OffHand, "offhand"),
        (Slot::Hand, "hand"),
        (Slot::Feet, "feet"),
        (Slot::Legs, "legs"),
        (Slot::Chest, "chest"),
        (Slot::Head, "head"),
        (Slot::Armor, "armor"),
        (Slot::Body, "body"),
        (Slot::Saddle, "saddle"),
    ];
    for (slot, slot_name) in SLOTS {
        let mut first = true;
        for modifier in modifiers
            .attribute_modifiers
            .iter()
            .filter(|m| std::mem::discriminant(&m.slot) == std::mem::discriminant(&slot))
        {
            let mut amount = modifier.amount;
            let base = match modifier.id {
                "minecraft:base_attack_damage" => Some(1.0),
                "minecraft:base_attack_speed" => Some(Attributes::ATTACK_SPEED.default_value),
                _ => None,
            };
            if let Some(base) = base {
                amount += base;
            }
            let operation = match modifier.operation {
                Operation::AddValue => 0,
                Operation::AddMultipliedBase => 1,
                Operation::AddMultipliedTotal => 2,
            };
            let name = modifier.r#type.name;
            let display = if operation != 0 {
                amount * 100.0
            } else if name == "minecraft:knockback_resistance" {
                amount * 10.0
            } else {
                amount
            };
            let attribute_key = format!("attribute.name.{}", name.trim_start_matches("minecraft:"));
            let attribute = lang.get_dynamic(&attribute_key);
            let (key, value, color) = if base.is_some() {
                ("equals", display, colors::DARK_GREEN)
            } else if amount > 0.0 {
                ("plus", display, sentiment_color(name, true))
            } else if amount < 0.0 {
                ("take", -display, sentiment_color(name, false))
            } else {
                continue;
            };
            if first {
                lines.push(StyledText::default());
                lines.push(StyledText::colored(
                    lang.get_dynamic(&format!("item.modifiers.{slot_name}")),
                    colors::GRAY,
                ));
                first = false;
            }
            let mut text = lang.format_dynamic(
                &format!("attribute.modifier.{key}.{operation}"),
                &[&modifier_number(value), attribute],
            );
            if base.is_some() {
                text.insert(0, ' ');
            }
            lines.push(StyledText::colored(&text, color));
        }
    }
}

/// `Attribute.getStyle`.
fn sentiment_color(attribute: &str, positive: bool) -> [f32; 3] {
    match attribute.trim_start_matches("minecraft:") {
        "gravity" | "scale" | "waypoint_transmit_range" | "waypoint_receive_range" => colors::GRAY,
        "burning_time" | "fall_damage_multiplier" => {
            if positive {
                colors::RED
            } else {
                colors::BLUE
            }
        }
        _ => {
            if positive {
                colors::BLUE
            } else {
                colors::RED
            }
        }
    }
}

/// `ItemAttributeModifiers.ATTRIBUTE_MODIFIER_FORMAT`: `#.##`.
fn modifier_number(value: f64) -> String {
    let text = format!("{:.2}", (value * 100.0).round_ties_even() / 100.0);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_numbers_read_like_vanilla() {
        assert_eq!(modifier_number(7.0), "7");
        assert_eq!(modifier_number(1.600_000_095_367_431_6), "1.6");
        assert_eq!(modifier_number(0.25), "0.25");
        assert_eq!(modifier_number(0.125), "0.12");
        assert_eq!(modifier_number(-0.001), "0");
    }

    #[test]
    fn hover_items_keep_their_enchantments() {
        let patch = serde_json::json!({"minecraft:enchantments": {"minecraft:sharpness": 5}});
        let stack = hover_stack("minecraft:diamond_sword", 1, Some(&patch)).unwrap();
        let lines: Vec<String> = tooltip_lines(&Lang::empty(), &stack)
            .iter()
            .map(StyledText::to_plain)
            .collect();
        assert!(
            lines[1].starts_with("enchantment.minecraft.sharpness"),
            "{lines:?}"
        );
        assert!(hover_stack("minecraft:nothing", 1, None).is_none());
    }

    #[test]
    fn a_custom_name_and_its_lore_keep_their_own_colours() {
        let patch = serde_json::json!({
            "minecraft:custom_name": {"text": "Blade", "color": "gold"},
            "minecraft:lore": [{"text": "old"}, {"text": "plain", "italic": false}],
        });
        let stack = hover_stack("minecraft:stick", 1, Some(&patch)).unwrap();
        let lines = tooltip_lines(&Lang::empty(), &stack);
        let name = &lines[0].spans[0].style;
        assert_eq!(name.color, Some(colors::GOLD));
        assert!(name.italic);
        assert_eq!(lines[1].to_plain(), "old");
        assert_eq!(lines[1].spans[0].style.color, Some(colors::DARK_PURPLE));
        assert!(lines[1].spans[0].style.italic);
        assert!(!lines[2].spans[0].style.italic);
    }

    #[test]
    fn a_named_player_head_says_whose_it_is() {
        let lang = Lang::from_pairs(&[("block.minecraft.player_head.named", "%s's Head")]);
        let patch = serde_json::json!({"minecraft:profile": "Notch"});
        let stack = hover_stack("minecraft:player_head", 1, Some(&patch)).unwrap();
        assert_eq!(display_name(&lang, &stack), "Notch's Head");
    }

    #[test]
    fn a_sword_lists_its_attack_in_the_main_hand() {
        let lang = Lang::empty();
        let sword = ItemStack::new(1, &pumpkin_data::item::Item::DIAMOND_SWORD);
        let lines: Vec<String> = tooltip_lines(&lang, &sword)
            .iter()
            .map(StyledText::to_plain)
            .collect();
        assert_eq!(
            lines[1..],
            [
                "",
                "item.modifiers.mainhand",
                " attribute.modifier.equals.0",
                " attribute.modifier.equals.0",
            ]
        );
    }
}
