//! What a right click on an entity comes to, as far as the client can tell.

use pumpkin_data::data_component_impl::{CustomNameImpl, DyeImpl};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{self, Taggable};

use super::appearance::Appearance;
use super::kind::EntityKind;

/// `InteractionResult` on the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityUse {
    /// Nothing: the item's own use comes next.
    Pass,
    /// `SUCCESS`: done, and the hand swings now.
    Swing,
    /// `CONSUME` or `SUCCESS_SERVER`.
    Quiet,
}

/// Who holds a lead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    /// The local player.
    Me,
    /// Another player.
    Player,
    /// A fence knot or a mob.
    Other,
}

/// The entity clicked, as the client knows it.
#[derive(Clone, Copy, Debug)]
pub struct Target<'a> {
    pub kind: EntityKind,
    pub appearance: &'a Appearance,
    pub saddled: bool,
    /// Who holds its lead, if it is on one.
    pub leashed_to: Option<Holder>,
    /// It holds the lead of another (a knot, a player, a mob).
    pub holds_leash: bool,
    /// Somebody already rides it.
    pub occupied: bool,
    /// A chest boat or a chest or hopper minecart: a click opens it.
    pub container: bool,
    /// An `Interaction`'s `DATA_RESPONSE_ID`.
    pub response: bool,
}

impl<'a> Target<'a> {
    /// An entity on no lead and holding none.
    pub fn new(kind: EntityKind, appearance: &'a Appearance) -> Self {
        Self {
            kind,
            appearance,
            saddled: false,
            leashed_to: None,
            holds_leash: false,
            occupied: false,
            container: false,
            response: false,
        }
    }
}

/// `Player.interactOn` with `held` in the clicking hand.
pub fn predict(target: Target, held: &ItemStack, sneaking: bool) -> EntityUse {
    if target.kind == EntityKind::LeashKnot {
        return EntityUse::Swing;
    }
    if target.kind == EntityKind::Interaction {
        return if target.response {
            EntityUse::Swing
        } else {
            EntityUse::Quiet
        };
    }
    let result = interact(target, held, sneaking);
    if result != EntityUse::Pass || held.is_empty() {
        return result;
    }
    item_on(target, held)
}

/// `Mob.interact` around `Entity.interact`; other entities only get the latter.
fn interact(target: Target, held: &ItemStack, sneaking: bool) -> EntityUse {
    let Target {
        kind,
        appearance,
        saddled,
        leashed_to,
        holds_leash,
        occupied,
        container,
        response: _,
    } = target;
    let item = held.item;
    let mob = kind.is_mob();
    if mob {
        if appearance.dying() {
            return EntityUse::Pass;
        }
        if is(item, &Item::NAME_TAG) && named(held) {
            return EntityUse::Swing;
        }
        if item.registry_key.ends_with("_spawn_egg") {
            return EntityUse::Quiet;
        }
    }
    let shears = is(item, &Item::SHEARS);
    if shears && (leashed_to.is_some() || holds_leash) {
        return EntityUse::Swing;
    }
    match kind {
        EntityKind::Boat => {
            return if !sneaking || container {
                EntityUse::Swing
            } else {
                EntityUse::Pass
            };
        }
        EntityKind::Minecart if container => return EntityUse::Swing,
        EntityKind::Minecart if !sneaking && !occupied => return EntityUse::Quiet,
        _ => {}
    }
    if !mob {
        return EntityUse::Pass;
    }
    if shears && saddled && !sneaking && !appearance.baby {
        return EntityUse::Swing;
    }
    match leashed_to {
        Some(Holder::Me) => return EntityUse::Swing,
        Some(Holder::Other) | None if is(item, &Item::LEAD) => return EntityUse::Quiet,
        _ => {}
    }
    mob_interact(kind, appearance, saddled, held, sneaking)
}

/// The mobs' own `mobInteract`.
fn mob_interact(
    kind: EntityKind,
    appearance: &Appearance,
    saddled: bool,
    held: &ItemStack,
    sneaking: bool,
) -> EntityUse {
    let item = held.item;
    match kind {
        EntityKind::Pig => {
            let food = item.has_tag(&tag::Item::MINECRAFT_PIG_FOOD);
            if !food && saddled && !sneaking {
                return EntityUse::Swing;
            }
            animal(appearance, food)
        }
        EntityKind::Cow if is(item, &Item::BUCKET) && !appearance.baby => EntityUse::Swing,
        EntityKind::Cow => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_COW_FOOD)),
        EntityKind::Sheep if is(item, &Item::SHEARS) => EntityUse::Quiet,
        EntityKind::Sheep => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_SHEEP_FOOD)),
        EntityKind::Chicken => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_CHICKEN_FOOD)),
        EntityKind::Fox => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_FOX_FOOD)),
        EntityKind::Frog => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_FROG_FOOD)),
        EntityKind::Camel => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_CAMEL_FOOD)),
        EntityKind::Armadillo => animal(
            appearance,
            item.has_tag(&tag::Item::MINECRAFT_ARMADILLO_FOOD),
        ),
        EntityKind::Bee => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_BEE_FOOD)),
        EntityKind::Goat => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_GOAT_FOOD)),
        EntityKind::Llama | EntityKind::TraderLlama => {
            animal(appearance, item.has_tag(&tag::Item::MINECRAFT_LLAMA_FOOD))
        }
        EntityKind::Panda => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_PANDA_FOOD)),
        EntityKind::Parrot => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_PARROT_FOOD)),
        EntityKind::Hoglin => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_HOGLIN_FOOD)),
        EntityKind::Strider => {
            let food = item.has_tag(&tag::Item::MINECRAFT_STRIDER_FOOD);
            if !food && saddled && !sneaking {
                return EntityUse::Swing;
            }
            animal(appearance, food)
        }
        EntityKind::Sniffer => animal(appearance, item.has_tag(&tag::Item::MINECRAFT_SNIFFER_FOOD)),
        EntityKind::CamelHusk => animal(
            appearance,
            item.has_tag(&tag::Item::MINECRAFT_CAMEL_HUSK_FOOD),
        ),
        EntityKind::Bogged if is(item, &Item::SHEARS) && !appearance.sheared() => EntityUse::Quiet,
        EntityKind::Creeper if item.has_tag(&tag::Item::MINECRAFT_CREEPER_IGNITERS) => {
            EntityUse::Swing
        }
        _ => EntityUse::Pass,
    }
}

/// `Animal.mobInteract`.
fn animal(appearance: &Appearance, food: bool) -> EntityUse {
    match (food, appearance.baby) {
        (false, _) => EntityUse::Pass,
        (true, true) => EntityUse::Swing,
        (true, false) => EntityUse::Quiet,
    }
}

/// `ItemStack.interactLivingEntity`: a saddle put on, a sheep dyed.
fn item_on(target: Target, held: &ItemStack) -> EntityUse {
    let Target {
        kind,
        appearance,
        saddled,
        ..
    } = target;
    if appearance.dying() {
        return EntityUse::Pass;
    }
    match kind {
        EntityKind::Pig if is(held.item, &Item::SADDLE) && !saddled && !appearance.baby => {
            EntityUse::Swing
        }
        EntityKind::Camel | EntityKind::CamelHusk | EntityKind::Strider
            if is(held.item, &Item::SADDLE) && !saddled && !appearance.baby =>
        {
            EntityUse::Swing
        }
        EntityKind::Sheep if !appearance.sheared() => match held.get_data_component::<DyeImpl>() {
            Some(dye) if dye.color != appearance.wool_color() => EntityUse::Swing,
            _ => EntityUse::Pass,
        },
        _ => EntityUse::Pass,
    }
}

fn is(item: &Item, other: &Item) -> bool {
    item.id == other.id
}

fn named(stack: &ItemStack) -> bool {
    stack.get_data_component::<CustomNameImpl>().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(kind: EntityKind, appearance: &Appearance) -> Target<'_> {
        Target::new(kind, appearance)
    }

    fn click(kind: EntityKind, appearance: &Appearance, item: &'static Item) -> EntityUse {
        predict(target(kind, appearance), &ItemStack::new(1, item), false)
    }

    #[test]
    fn a_bucket_milks_a_cow_but_not_a_calf_or_a_pig() {
        let adult = Appearance::default();
        let calf = Appearance {
            baby: true,
            ..Default::default()
        };
        assert_eq!(
            click(EntityKind::Cow, &adult, &Item::BUCKET),
            EntityUse::Swing
        );
        assert_eq!(
            click(EntityKind::Cow, &calf, &Item::BUCKET),
            EntityUse::Pass
        );
        assert_eq!(
            click(EntityKind::Pig, &adult, &Item::BUCKET),
            EntityUse::Pass
        );
    }

    #[test]
    fn food_feeds_babies_at_once_and_grown_ones_on_the_server() {
        let adult = Appearance::default();
        let baby = Appearance {
            baby: true,
            ..Default::default()
        };
        assert_eq!(
            click(EntityKind::Cow, &adult, &Item::WHEAT),
            EntityUse::Quiet
        );
        assert_eq!(
            click(EntityKind::Sheep, &baby, &Item::WHEAT),
            EntityUse::Swing
        );
        assert_eq!(
            click(EntityKind::Pig, &adult, &Item::CARROT),
            EntityUse::Quiet
        );
        assert_eq!(
            click(EntityKind::Chicken, &adult, &Item::WHEAT_SEEDS),
            EntityUse::Quiet
        );
        assert_eq!(
            click(EntityKind::Pig, &adult, &Item::WHEAT),
            EntityUse::Pass
        );
    }

    #[test]
    fn a_saddled_pig_is_ridden_unless_sneaking() {
        let pig = Appearance::default();
        let saddled = Target {
            saddled: true,
            ..target(EntityKind::Pig, &pig)
        };
        let hand = ItemStack::EMPTY;
        assert_eq!(predict(saddled, hand, false), EntityUse::Swing);
        assert_eq!(predict(saddled, hand, true), EntityUse::Pass);
        assert_eq!(
            click(EntityKind::Pig, &pig, &Item::SADDLE),
            EntityUse::Swing
        );
        assert_eq!(
            predict(saddled, &ItemStack::new(1, &Item::SADDLE), true),
            EntityUse::Pass
        );
    }

    #[test]
    fn dye_colours_a_sheep_of_another_colour_only() {
        let white = Appearance::default();
        let sheared = Appearance {
            wool: 0x10,
            ..Default::default()
        };
        assert_eq!(
            click(EntityKind::Sheep, &white, &Item::RED_DYE),
            EntityUse::Swing
        );
        assert_eq!(
            click(EntityKind::Sheep, &white, &Item::WHITE_DYE),
            EntityUse::Pass
        );
        assert_eq!(
            click(EntityKind::Sheep, &sheared, &Item::RED_DYE),
            EntityUse::Pass
        );
        assert_eq!(
            click(EntityKind::Sheep, &white, &Item::SHEARS),
            EntityUse::Quiet
        );
    }

    #[test]
    fn creepers_light_and_monsters_or_players_pass() {
        let any = Appearance::default();
        assert_eq!(
            click(EntityKind::Creeper, &any, &Item::FLINT_AND_STEEL),
            EntityUse::Swing
        );
        assert_eq!(
            click(EntityKind::Zombie, &any, &Item::STICK),
            EntityUse::Pass
        );
        assert_eq!(
            click(EntityKind::Player, &any, &Item::LEAD),
            EntityUse::Pass
        );
        assert_eq!(
            click(EntityKind::Spider, &any, &Item::LEAD),
            EntityUse::Quiet
        );
        assert_eq!(
            click(EntityKind::Skeleton, &any, &Item::PIG_SPAWN_EGG),
            EntityUse::Quiet
        );
    }

    #[test]
    fn a_lead_ties_on_unties_from_me_and_leaves_others_leads() {
        let cow = Appearance::default();
        let lead = ItemStack::new(1, &Item::LEAD);
        let on = |leashed_to| Target {
            leashed_to,
            ..target(EntityKind::Cow, &cow)
        };
        assert_eq!(predict(on(None), &lead, false), EntityUse::Quiet);
        assert_eq!(
            predict(on(Some(Holder::Other)), &lead, false),
            EntityUse::Quiet
        );
        assert_eq!(
            predict(on(Some(Holder::Player)), &lead, false),
            EntityUse::Pass
        );
        assert_eq!(
            predict(on(Some(Holder::Me)), ItemStack::EMPTY, false),
            EntityUse::Swing
        );
    }

    #[test]
    fn shears_cut_leads_on_anything_and_knots_always_react() {
        let any = Appearance::default();
        let shears = ItemStack::new(1, &Item::SHEARS);
        let holding = Target {
            holds_leash: true,
            ..target(EntityKind::Player, &any)
        };
        assert_eq!(predict(holding, &shears, false), EntityUse::Swing);
        assert_eq!(
            click(EntityKind::Player, &any, &Item::SHEARS),
            EntityUse::Pass
        );
        let leashed = Target {
            leashed_to: Some(Holder::Other),
            ..target(EntityKind::Zombie, &any)
        };
        assert_eq!(predict(leashed, &shears, false), EntityUse::Swing);
        assert_eq!(
            click(EntityKind::LeashKnot, &any, &Item::STICK),
            EntityUse::Swing
        );
    }

    #[test]
    fn the_dying_do_not_react() {
        let dying = Appearance {
            health: 0.0,
            ..Default::default()
        };
        assert_eq!(
            click(EntityKind::Cow, &dying, &Item::BUCKET),
            EntityUse::Pass
        );
    }
}
