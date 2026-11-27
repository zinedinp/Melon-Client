//! Which model a stack shows ([`ItemKey`]): its item and what its
//! components change about the model ([`Variant`]). Looked up for every
//! drawn stack, every frame.

use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::{DamageImpl, MaxDamageImpl};
use pumpkin_data::item_stack::ItemStack;

use super::{COUNT_CUTS, Count, count_cuts, definition_id, potion_color};

/// Which model a stack shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ItemKey {
    pub item: u16,
    /// What the stack's components add (`item_model`, `custom_model_data`, ...).
    pub variant: Option<Arc<Variant>>,
    pub potion_color: Option<[u8; 3]>,
    /// The stack size as far as the item's model tells sizes apart.
    pub count: u8,
    pub max_stack: u8,
}

/// Bumped when the count cuts are forgotten: every cached key goes stale.
static RESOURCES: AtomicU32 = AtomicU32::new(0);

pub(super) fn resources_changed() {
    RESOURCES.fetch_add(1, Ordering::Relaxed);
}

/// A stack, and what drawing it asks of it every frame.
pub trait StackKey {
    fn stack(&self) -> &ItemStack;
    fn item_key(&self, assets_root: &Path) -> ItemKey;

    /// `ItemStack.hasFoil`.
    fn foil(&self) -> bool {
        crate::glint::has_foil(self.stack())
    }

    /// Whether it is a player head with a profile, drawn with its skin.
    fn has_head(&self) -> bool {
        crate::head_profile::HeadProfile::of_stack(self.stack()).is_some()
    }
}

impl StackKey for ItemStack {
    fn stack(&self) -> &ItemStack {
        self
    }

    fn item_key(&self, assets_root: &Path) -> ItemKey {
        ItemKey::of(assets_root, self)
    }
}

/// A flag worked out on first use: 0 not yet, else `1 + value`.
#[derive(Default)]
struct LazyFlag(AtomicU8);

impl LazyFlag {
    fn get(&self, work_out: impl FnOnce() -> bool) -> bool {
        match self.0.load(Ordering::Relaxed) {
            0 => {
                let value = work_out();
                self.0.store(1 + u8::from(value), Ordering::Relaxed);
                value
            }
            seen => seen == 2,
        }
    }

    fn copy(&self) -> Self {
        Self(AtomicU8::new(self.0.load(Ordering::Relaxed)))
    }
}

/// A stack drawn every frame (slots, dropped, framed, displayed) with its
/// [`ItemKey`], glint and head check, worked out once until the stack (or,
/// for the key, the resources) change. Any mutable access to the stack drops them.
pub struct KeyedStack {
    stack: ItemStack,
    key: Mutex<Option<(u32, ItemKey)>>,
    foil: LazyFlag,
    head: LazyFlag,
}

impl KeyedStack {
    pub fn new(stack: ItemStack) -> Self {
        Self {
            stack,
            key: Mutex::new(None),
            foil: LazyFlag::default(),
            head: LazyFlag::default(),
        }
    }

    pub fn set(&mut self, stack: ItemStack) {
        *self = Self::new(stack);
    }

    /// The stack, leaving an empty one.
    pub fn take(&mut self) -> ItemStack {
        std::mem::replace(&mut **self, ItemStack::EMPTY.clone())
    }
}

impl Default for KeyedStack {
    fn default() -> Self {
        Self::new(ItemStack::EMPTY.clone())
    }
}

impl From<ItemStack> for KeyedStack {
    fn from(stack: ItemStack) -> Self {
        Self::new(stack)
    }
}

impl std::ops::Deref for KeyedStack {
    type Target = ItemStack;

    fn deref(&self) -> &ItemStack {
        &self.stack
    }
}

impl std::ops::DerefMut for KeyedStack {
    fn deref_mut(&mut self) -> &mut ItemStack {
        *self.key.get_mut().unwrap_or_else(PoisonError::into_inner) = None;
        self.foil = LazyFlag::default();
        self.head = LazyFlag::default();
        &mut self.stack
    }
}

impl Clone for KeyedStack {
    fn clone(&self) -> Self {
        let key = self
            .key
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        Self {
            stack: self.stack.clone(),
            key: Mutex::new(key),
            foil: self.foil.copy(),
            head: self.head.copy(),
        }
    }
}

impl StackKey for KeyedStack {
    fn stack(&self) -> &ItemStack {
        &self.stack
    }

    fn item_key(&self, assets_root: &Path) -> ItemKey {
        let resources = RESOURCES.load(Ordering::Relaxed);
        let mut cached = self.key.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((seen, key)) = &*cached
            && *seen == resources
        {
            return key.clone();
        }
        let key = ItemKey::of(assets_root, &self.stack);
        *cached = Some((resources, key.clone()));
        key
    }

    fn foil(&self) -> bool {
        self.foil.get(|| crate::glint::has_foil(&self.stack))
    }

    fn has_head(&self) -> bool {
        self.head
            .get(|| crate::head_profile::HeadProfile::of_stack(&self.stack).is_some())
    }
}

impl ItemKey {
    pub fn of(assets_root: &Path, stack: &ItemStack) -> Self {
        let max_stack = stack.get_max_stack_size();
        let variant = Variant::of(stack).map(Arc::new);
        Self {
            item: stack.item.id,
            potion_color: potion_color(stack),
            count: model_count(assets_root, stack, max_stack, variant.as_deref()),
            max_stack,
            variant,
        }
    }

    /// The count the model is resolved with.
    pub fn count(&self) -> Count {
        Count {
            value: i32::from(self.count),
            max_stack: i32::from(self.max_stack),
        }
    }
}

/// The components of a stack that pick its model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Variant {
    /// The definition named by `item_model`, when it isn't the item's own.
    pub model: Option<String>,
    /// `CustomModelData.floats`, as bits to be hashable.
    pub floats: Vec<u32>,
    pub flags: Vec<bool>,
    pub strings: Vec<String>,
    pub colors: Vec<i32>,
    pub dye: Option<i32>,
    /// `ItemStack.nextDamageWillBreak`: the `minecraft:broken` condition.
    pub broken: bool,
    /// `ItemStack.has`: bit `1 << DataComponent::to_id()`.
    pub present: u128,
    /// `ItemStack.hasNonDefault`: components in the patch.
    pub patched: u128,
    /// `minecraft:trim` material (`SelectItemModel` `trim_material`).
    pub trim_material: Option<String>,
    /// The fields' hash, taken once in [`Self::of`]: a lookup hashes this
    /// instead of the strings and lists.
    pub(super) hash: u64,
}

impl Hash for Variant {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl Variant {
    /// `None` for a stack with none of these components.
    pub fn of(stack: &ItemStack) -> Option<Self> {
        use pumpkin_data::data_component_impl::{
            CustomModelDataImpl, DyedColorImpl, ItemModelImpl, TrimImpl,
        };
        let own = stack.item.registry_key;
        // Every item has an `item_model`, mostly its own: compared before
        // the id is copied, as `trim_start_matches` would (every prefix).
        let model = stack
            .get_data_component::<ItemModelImpl>()
            .filter(|m| {
                let mut id = &*m.id;
                while let Some(rest) = id.strip_prefix("minecraft:") {
                    id = rest;
                }
                id != own
            })
            .map(|m| m.id.to_string());
        let data = stack.get_data_component::<CustomModelDataImpl>();
        let dye = stack.get_data_component::<DyedColorImpl>().map(|d| d.rgb);
        let trim_material = stack
            .get_data_component::<TrimImpl>()
            .and_then(|trim| trim.material_enum())
            .map(|material| material.asset_id().to_owned());
        let broken = next_damage_will_break(stack);
        let (present, patched) = component_bits(stack);
        if model.is_none()
            && data.is_none()
            && dye.is_none()
            && trim_material.is_none()
            && !broken
            && patched == 0
        {
            return None;
        }
        let mut variant = Self {
            model,
            dye,
            broken,
            present,
            patched,
            trim_material,
            ..Self::default()
        };
        if let Some(data) = data {
            variant.floats = data.floats.iter().map(|f| f.to_bits()).collect();
            variant.flags.clone_from(&data.flags);
            variant.strings.clone_from(&data.strings);
            variant.colors.clone_from(&data.colors);
        }
        variant.hash = variant.content_hash();
        Some(variant)
    }

    /// Every field but `hash`.
    fn content_hash(&self) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        (
            &self.model,
            &self.floats,
            &self.flags,
            &self.strings,
            &self.colors,
            self.dye,
            self.broken,
            self.present,
            self.patched,
            &self.trim_material,
        )
            .hash(&mut hasher);
        hasher.finish()
    }

    /// `CustomModelData.getFloat`.
    pub(super) fn float(&self, index: usize) -> Option<f32> {
        self.floats.get(index).map(|&bits| f32::from_bits(bits))
    }
}

/// A lock and a hash of the item's name per call: stacks drawn every
/// frame keep their key ([`KeyedStack`]).
fn model_count(
    assets_root: &Path,
    stack: &ItemStack,
    max_stack: u8,
    variant: Option<&Variant>,
) -> u8 {
    let definition_key = definition_id(stack.item.registry_key, variant);
    let mut known = COUNT_CUTS.lock().unwrap_or_else(|e| e.into_inner());
    let known = known.get_or_insert_with(Default::default);
    let count = i32::from(stack.item_count);
    let pick = |cuts: &[i32]| {
        cuts.iter()
            .rev()
            .find(|&&cut| cut <= count)
            .map_or(1, |&cut| cut.clamp(1, 255) as u8)
    };
    // Looked up by `&str`, once: the key is only allocated the first time
    // an item's definition is read, not on every call.
    if let Some(cuts) = known.get(definition_key) {
        return pick(cuts);
    }
    let definition = melon_assets::assets::json_resource(assets_root, "items", definition_key);
    let mut cuts = Vec::new();
    if let Some(definition) = definition {
        count_cuts(&definition["model"], i32::from(max_stack), &mut cuts);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let picked = pick(&cuts);
    known.insert(definition_key.to_owned(), cuts);
    picked
}

/// `(present, patched)`: the components the stack has (`ItemStack.has`,
/// its patch over its item's defaults) and those its patch sets
/// (`hasNonDefault`), one bit per [`DataComponent::to_id`].
///
/// One pass over the patch and one over the defaults, where asking
/// `has_data_component` for every component scanned both lists per id.
fn component_bits(stack: &ItemStack) -> (u128, u128) {
    let mut present = 0u128;
    let mut patched = 0u128;
    // What the patch decided: `has_data_component` answers from the first
    // patch entry for an id and only then asks the defaults.
    let mut decided = 0u128;
    for (id, data) in &stack.patch {
        let bit = component_bit(*id);
        if decided & bit == 0 {
            decided |= bit;
            if data.is_some() {
                present |= bit;
            }
        }
        if data.is_some() {
            patched |= bit;
        }
    }
    for &(id, _) in stack.item.components {
        present |= component_bit(id) & !decided;
    }
    (present, patched)
}

/// The bit of `id`; every vanilla component (up to `CushionColor`, 121)
/// fits a `u128`.
fn component_bit(id: DataComponent) -> u128 {
    1u128 << id.to_id()
}

/// `ItemStack.nextDamageWillBreak`: one more hit and the stack is gone.
fn next_damage_will_break(stack: &ItemStack) -> bool {
    stack
        .get_data_component::<MaxDamageImpl>()
        .is_some_and(|max| {
            let damage = stack
                .get_data_component::<DamageImpl>()
                .map_or(0, |d| d.damage);
            damage >= max.max_damage - 1
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::data_component_impl::{DataComponentImpl, DyedColorImpl};
    use pumpkin_data::item::Item;

    /// What `component_bits` worked out before: ask every component.
    fn component_bits_by_asking(stack: &ItemStack) -> (u128, u128) {
        let mut present = 0u128;
        let mut patched = 0u128;
        for id in 0u8..=DataComponent::CushionColor.to_id() {
            let Some(component) = DataComponent::try_from_id(id) else {
                continue;
            };
            if stack.has_data_component(component) {
                present |= 1u128 << id;
            }
        }
        for (id, data) in &stack.patch {
            if data.is_some() {
                patched |= 1u128 << id.to_id();
            }
        }
        (present, patched)
    }

    fn set(
        component: DataComponent,
        data: impl DataComponentImpl + 'static,
    ) -> (DataComponent, Option<Box<dyn DataComponentImpl>>) {
        (component, Some(Box::new(data)))
    }

    #[test]
    fn a_keyed_stack_keeps_its_key_until_it_or_the_resources_change() {
        let root = Path::new("no-assets");
        let mut stack = KeyedStack::new(ItemStack::new(1, &Item::DIAMOND_SWORD));
        let first = stack.item_key(root);
        assert_eq!(first, ItemKey::of(root, &stack));
        assert_eq!(stack.item_key(root), first);
        let mut damaged = ItemStack::new(1, &Item::DIAMOND_SWORD);
        damaged
            .patch
            .push(set(DataComponent::Damage, DamageImpl { damage: 3 }));
        stack.set(damaged);
        assert_ne!(stack.item_key(root), first);
        assert_eq!(stack.item_key(root), ItemKey::of(root, &stack));
        resources_changed();
        assert_eq!(stack.item_key(root), ItemKey::of(root, &stack));
    }

    #[test]
    fn equal_stacks_hash_their_variant_alike_and_others_apart() {
        use std::hash::BuildHasher;
        let dyed = |rgb| {
            let mut stack = ItemStack::new(1, &Item::LEATHER_HELMET);
            stack
                .patch
                .push(set(DataComponent::DyedColor, DyedColorImpl { rgb }));
            stack
        };
        let hash = |stack: &ItemStack| {
            rustc_hash::FxBuildHasher.hash_one(ItemKey::of(Path::new("no-assets"), stack))
        };
        assert_eq!(hash(&dyed(7)), hash(&dyed(7)));
        assert_ne!(hash(&dyed(7)), hash(&dyed(8)));
        let (a, b) = (Variant::of(&dyed(7)), Variant::of(&dyed(7)));
        assert_eq!(a, b);
        assert_ne!(a.map(|v| v.hash), Some(0));
    }

    #[test]
    fn a_keyed_stack_forgets_its_glint_when_the_stack_changes() {
        use pumpkin_data::data_component_impl::EnchantmentGlintOverrideImpl;
        let mut stack = KeyedStack::new(ItemStack::new(1, &Item::DIAMOND_SWORD));
        assert!(!stack.foil());
        assert!(!stack.has_head());
        stack.patch.push(set(
            DataComponent::EnchantmentGlintOverride,
            EnchantmentGlintOverrideImpl { glint: true },
        ));
        assert!(stack.foil());
        assert!(stack.clone().foil());
    }

    #[test]
    fn component_bits_match_asking_for_every_component() {
        let plain = ItemStack::new(1, &Item::DIAMOND_SWORD);
        let mut damaged = ItemStack::new(1, &Item::DIAMOND_SWORD);
        damaged
            .patch
            .push(set(DataComponent::Damage, DamageImpl { damage: 3 }));
        let mut unbreakable = ItemStack::new(1, &Item::DIAMOND_SWORD);
        unbreakable.patch.push((DataComponent::MaxDamage, None));
        let mut twice = ItemStack::new(1, &Item::LEATHER_HELMET);
        twice.patch.push(set(
            DataComponent::DyedColor,
            DyedColorImpl { rgb: 0x00FF_0000 },
        ));
        twice.patch.push((DataComponent::DyedColor, None));
        let mut removed_then_set = ItemStack::new(1, &Item::LEATHER_HELMET);
        removed_then_set
            .patch
            .push((DataComponent::MaxStackSize, None));
        removed_then_set
            .patch
            .push(set(DataComponent::DyedColor, DyedColorImpl { rgb: 7 }));
        for stack in [&plain, &damaged, &unbreakable, &twice, &removed_then_set] {
            assert_eq!(
                component_bits(stack),
                component_bits_by_asking(stack),
                "{}",
                stack.item.registry_key
            );
        }
        let max_damage = component_bit(DataComponent::MaxDamage);
        assert_ne!(component_bits(&plain).0 & max_damage, 0);
        assert_eq!(component_bits(&unbreakable).0 & max_damage, 0);
        let dyed = component_bit(DataComponent::DyedColor);
        assert_ne!(component_bits(&twice).0 & dyed, 0);
        assert_ne!(component_bits(&twice).1 & dyed, 0);
    }
}
