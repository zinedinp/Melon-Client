//! Vanilla `Minecraft.handleKeybinds` for attack/use/pick, and

use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::{Block, BlockStateId};

use super::{InWorld, sounds};
use crate::audio::block_sounds::BlockSound;
use crate::game::controls::WorldContext;
use crate::gui::overlay::container::ContainerOverlay;
use crate::gui::overlay::{ChatExit, OverlayKind};
use crate::network::play::SWAP_CHANNEL;
use crate::network::play::game::PlayerAction;
use crate::world::block_placement;
use crate::world::entity::interact::{self as entity_interact, EntityUse, Holder};
use crate::world::entity::kind::EntityKind;
use crate::world::inventory::{ClickType, OFFHAND_SLOT, empty};
use crate::world::pick::{self, BlockHit, EntityHit, Hit};
use crate::world::player::hands::{Hand, ItemUse, Swing, UseAnimation, UsingItem};
use crate::world::player::mining::{Miner, breaks_blocks_in_creative, enchantment_level};
use pumpkin_data::data_component_impl::PiercingWeaponImpl;

/// `TridentItem.releaseUsing`: the least ticks held for a throw or a spin.
const TRIDENT_MIN_HOLD: i32 = 10;

/// `InventoryMenu`'s helmet slot, for Aqua Affinity.
const HEAD_SLOT: usize = 5;

/// `MultiPlayerGameMode.destroyDelay` after a creative break or a finished
const DESTROY_DELAY: i32 = 5;
/// `Minecraft.rightClickDelay` after each use.
const RIGHT_CLICK_DELAY: i32 = 4;
/// `Minecraft.missTime` after punching at nothing.
const MISS_TIME: i32 = 10;
/// `Minecraft.missTime` while a screen is open: no attack starts until the
/// key is let go (`continueAttack(false)` clears it).
pub(super) const SCREEN_MISS_TIME: i32 = 10_000;

/// The attack and use keys' state between ticks: `Minecraft`'s delays and
/// `MultiPlayerGameMode`'s block breaking.
#[derive(Default)]
pub(super) struct Interaction {
    /// `MultiPlayerGameMode.isDestroying` with the block and its progress.
    pub(super) destroying: Option<DestroyTarget>,
    /// `MultiPlayerGameMode.destroyDelay`.
    pub(super) destroy_delay: i32,
    /// `Minecraft.missTime`: after a punch at nothing, the attack key rests.
    pub(super) miss_time: i32,
    /// `Minecraft.rightClickDelay`.
    pub(super) right_click_delay: i32,
}

/// What a hand's try at a right click came to (`InteractionResult`), as
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UseOutcome {
    /// Nothing happened: the next try goes on.
    Pass,
    /// Something happened, or it failed for good: the click is over.
    Done { swing: bool, used: bool },
}

pub(super) struct DestroyTarget {
    pos: [i32; 3],
    face: u8,
    progress: f32,
    /// `destroyTicks`: the hit sound plays every fourth.
    ticks: u32,
    item: ItemStack,
}

impl InWorld {
    pub(super) fn next_sequence(&mut self) -> i32 {
        self.predictions.start_predicting()
    }

    /// Opens or closes the overlay for [`Self::open_container`] once a tick
    pub(super) fn apply_pending_container_overlay(&mut self, ui: &WorldContext) {
        if self.menus.pending_container_close {
            self.menus.pending_container_close = false;
            if matches!(
                self.overlay_kind(),
                Some(OverlayKind::Inventory | OverlayKind::Creative | OverlayKind::Container)
            ) {
                self.finish_overlay(ChatExit::Interrupted, ui, false);
            }
            self.menus.open_container = None;
        }
        if !self.menus.pending_container_overlay {
            return;
        }
        self.menus.pending_container_overlay = false;
        let Some(kind) = self.menus.open_container.as_ref().map(|menu| menu.kind) else {
            return;
        };
        if self.overlay_kind() == Some(OverlayKind::Container) {
            self.keys.clear_clicks();
            let overlay = ContainerOverlay::new(ui.size.0, ui.size.1, kind);
            self.overlay = Some(Box::new(overlay));
            let (x, y) = self.keys.mouse;
            self.with_overlay(ui, |overlay, ctx| overlay.mouse_moved(x, y, false, ctx));
            return;
        }
        let was_open = self.overlay.is_some();
        if was_open {
            self.finish_overlay(ChatExit::Interrupted, ui, false);
        }
        let overlay = ContainerOverlay::new(ui.size.0, ui.size.1, kind);
        self.open_overlay_with(Box::new(overlay), !was_open, ui);
    }

    /// `Minecraft.handleKeybinds` for attack, use and pick. While an item
    pub(super) fn handle_keybinds(&mut self, _ui: &WorldContext) {
        self.hit_result = self.pick_hit(self.player.tick_eye());
        let mut instant = false;
        if self.hands.using.is_some() {
            if !self.keys.use_item {
                self.release_using_item();
            }
            while self.keys.consume_attack() {}
            while self.keys.consume_use() {}
            while self.keys.consume_pick() {}
        } else {
            if self.try_swap() {
                return;
            }
            while self.keys.consume_attack() {
                instant |= self.start_attack();
            }
            while self.keys.consume_use() {
                self.start_use_item();
            }
            while self.keys.consume_pick() {
                self.pick_block();
            }
        }
        if self.keys.use_item
            && self.interaction.right_click_delay == 0
            && self.hands.using.is_none()
        {
            self.start_use_item();
        }
        self.continue_attack(self.keys.attack && !instant);
    }

    /// `MultiPlayerGameMode.getDestroyStage` into `destroyBlockProgress`:
    pub(super) fn update_own_crack(&mut self) {
        let own = self.joined.session.info.entity_id;
        match &self.interaction.destroying {
            Some(target) => {
                let stage = if target.progress > 0.0 {
                    (target.progress * 10.0) as i32
                } else {
                    -1
                };
                self.cracks.set(own, target.pos, stage, self.ticks);
            }
            None => self.cracks.set(own, [0; 3], -1, self.ticks),
        }
    }

    /// `MultiPlayerGameMode.releaseUsingItem`: lets go of a shield, or of
    pub(super) fn release_using_item(&mut self) {
        if let Some(using) = self.hands.using.take() {
            self.predict_riptide(&using);
            self.joined
                .session
                .send_player_action(PlayerAction::ReleaseUseItem);
        }
    }

    /// `TridentItem.releaseUsing` on the client: a riptide trident let go
    fn predict_riptide(&mut self, using: &UsingItem) {
        if using.item_use.animation != UseAnimation::Trident
            || using.item_use.duration - using.remaining < TRIDENT_MIN_HOLD
            || !self.player.in_water()
            || self.player.riding()
        {
            return;
        }
        let level = enchantment_level(self.hand_stack(using.hand), "minecraft:riptide");
        if level > 0 {
            self.player.start_riptide(&self.level, level as u32);
        }
    }

    /// `LocalPlayer.onSyncedDataUpdated` for the living flags: the server
    pub(super) fn sync_using_item(&mut self, living: u8) {
        use crate::network::play::game::{LIVING_OFF_HAND, LIVING_USING_ITEM};
        let using = living & LIVING_USING_ITEM != 0;
        if !using {
            self.hands.using = None;
            return;
        }
        if self.hands.using.is_some() {
            return;
        }
        let hand = if living & LIVING_OFF_HAND != 0 {
            Hand::Off
        } else {
            Hand::Main
        };
        if let Some(item_use) = ItemUse::of(self.hand_stack(hand)) {
            self.hands.using = Some(UsingItem {
                hand,
                item_use,
                remaining: item_use.duration,
                overrun: 0,
            });
        }
    }

    /// The stack in `hand`.
    pub(super) fn hand_stack(&self, hand: Hand) -> &ItemStack {
        match hand {
            Hand::Main => self.inventory.selected_stack(),
            Hand::Off => &self.inventory.slots()[OFFHAND_SLOT],
        }
    }

    /// Swings `hand` as its item does, faster with Haste and slower with
    fn swing(&mut self, hand: Hand) {
        let mut swing = Swing::of(self.hand_stack(hand));
        swing.duration = self.mining_effects.swing_duration(swing.duration);
        self.hands.swing.start_if_able(hand, swing);
    }

    /// What breaking speed depends on: the main hand, the helmet, the
    fn miner(&self) -> Miner<'_> {
        Miner {
            held: self.inventory.selected_stack(),
            helmet: &self.inventory.slots()[HEAD_SLOT],
            effects: self.mining_effects,
            efficiency: self.player.attributes().mining_efficiency(),
            break_speed: self.player.attributes().block_break_speed(),
            eye_in_water: crate::world::physics::eye_in_water(&self.level, self.player.tick_eye()),
            on_ground: self.player.on_ground(),
        }
    }

    pub(super) fn abort_destroy(&mut self) {
        if let Some(target) = self.interaction.destroying.take() {
            self.joined.session.send_player_action_at(
                PlayerAction::AbortDestroyBlock,
                target.pos,
                pick::FACE_DOWN,
                0,
            );
        }
    }

    /// The block looked at this tick, unless an entity is in front of it.
    pub(super) fn hit_block(&self) -> Option<BlockHit> {
        match self.hit_result {
            Some(Hit::Block(hit)) => Some(hit),
            _ => None,
        }
    }

    /// The entity looked at this tick.
    pub(super) fn hit_entity(&self) -> Option<EntityHit> {
        match self.hit_result {
            Some(Hit::Entity(hit)) => Some(hit),
            _ => None,
        }
    }

    /// `Hud.extractCrosshair`'s `renderMaxAttackIndicator`: a creature that
    pub(super) fn attack_target_ready(&self) -> bool {
        let Some(hit) = self.hit_entity() else {
            return false;
        };
        let living = self
            .entities
            .get(hit.id)
            .is_some_and(|entity| entity.is_living() && !entity.appearance.dying());
        living && crate::world::player::hands::attack_delay(self.inventory.selected_stack()) > 5.0
    }

    /// What the crosshair ray from `eye` meets first: a block or an entity.
    pub(super) fn pick_hit(&self, eye: glam::DVec3) -> Option<Hit> {
        let (block_range, entity_range) = if self.game_mode.current.has_infinite_materials() {
            (pick::CREATIVE_RANGE, pick::CREATIVE_ENTITY_RANGE)
        } else {
            (pick::SURVIVAL_RANGE, pick::SURVIVAL_ENTITY_RANGE)
        };
        let dir = self.player.forward_vector().as_dvec3();
        let boxes = self.entities.iter().filter_map(|entity| {
            let spectating = entity.kind == EntityKind::Player
                && self
                    .players
                    .get(&entity.uuid)
                    .is_some_and(|p| p.game_mode.is_spectator());
            if spectating {
                return None;
            }
            Some((entity.id, entity.pick_box()?))
        });
        pick::pick(&self.level, eye, dir, block_range, entity_range, boxes)
    }

    /// `Minecraft.startAttack`: hits the entity looked at, starts breaking
    fn start_attack(&mut self) -> bool {
        if self.interaction.miss_time > 0 {
            return false;
        }
        if self.game_mode.current.is_spectator() {
            let target = self.hit_entity().map(|hit| hit.id);
            self.joined.session.send_spectate(target);
            return true;
        }
        if self
            .inventory
            .selected_stack()
            .get_data_component::<PiercingWeaponImpl>()
            .is_some()
        {
            self.joined.session.send_player_action(PlayerAction::Stab);
            self.hands.reset_attack_strength();
            self.swing(Hand::Main);
            return false;
        }
        let mut broke = false;
        if let Some(hit) = self.hit_entity() {
            self.attack(hit.id);
            self.swing(Hand::Main);
            self.joined.session.send_punch();
            return false;
        }
        match self.hit_block().filter(|hit| !self.is_air(hit.pos)) {
            Some(hit) => {
                self.start_destroy(hit.pos, hit.face);
                broke = self.is_air(hit.pos);
            }
            None => {
                if !self.game_mode.current.has_infinite_materials() {
                    self.interaction.miss_time = MISS_TIME;
                }
                self.hands.reset_attack_strength();
            }
        }
        self.swing(Hand::Main);
        self.joined.session.send_punch();
        broke
    }

    /// `Minecraft.continueAttack`: holding attack on a block keeps breaking
    fn continue_attack(&mut self, down: bool) {
        if !down {
            self.interaction.miss_time = 0;
        }
        if self.interaction.miss_time > 0 || self.hands.using.is_some() {
            return;
        }
        let hit = self
            .hit_block()
            .filter(|hit| down && !self.is_air(hit.pos) && !self.game_mode.current.is_spectator());
        match hit {
            Some(hit) => {
                self.continue_destroy(hit.pos, hit.face);
                self.swing(Hand::Main);
                self.joined.session.send_punch();
            }
            None => self.abort_destroy(),
        }
    }

    /// `MultiPlayerGameMode.attack`: tells the server, which works out the
    fn attack(&mut self, entity: i32) {
        self.joined.session.send_attack(entity);
        self.hands.reset_attack_strength();
        if self.game_mode.current.has_infinite_materials() {
            self.interaction.destroy_delay = DESTROY_DELAY;
        }
    }

    /// `MultiPlayerGameMode.destroyBlock` in creative: the block goes at
    fn destroy_block_in_creative(&mut self, pos: [i32; 3]) {
        if breaks_blocks_in_creative(self.inventory.selected_stack()) {
            self.destroy_block(pos);
        }
    }

    fn start_destroy(&mut self, pos: [i32; 3], face: u8) {
        if self.game_mode.current.has_infinite_materials() {
            let sequence = self.next_sequence();
            self.destroy_block_in_creative(pos);
            self.joined.session.send_player_action_at(
                PlayerAction::StartDestroyBlock,
                pos,
                face,
                sequence,
            );
            self.interaction.destroy_delay = DESTROY_DELAY;
            return;
        }
        if self.same_destroy_target(pos) {
            return;
        }
        if let Some(old) = self.interaction.destroying.take() {
            self.joined.session.send_player_action_at(
                PlayerAction::AbortDestroyBlock,
                old.pos,
                face,
                0,
            );
        }
        let progress = self.miner().destroy_progress(self.block_at(pos));
        let sequence = self.next_sequence();
        if progress >= 1.0 {
            self.destroy_block(pos);
        } else if !self.is_air(pos) {
            self.interaction.destroying = Some(DestroyTarget {
                pos,
                face,
                progress: 0.0,
                ticks: 0,
                item: self.inventory.selected_stack().clone(),
            });
        }
        self.joined.session.send_player_action_at(
            PlayerAction::StartDestroyBlock,
            pos,
            face,
            sequence,
        );
    }

    fn continue_destroy(&mut self, pos: [i32; 3], face: u8) {
        if self.interaction.destroy_delay > 0 {
            self.interaction.destroy_delay -= 1;
            return;
        }
        if self.game_mode.current.has_infinite_materials() {
            self.interaction.destroy_delay = DESTROY_DELAY;
            let sequence = self.next_sequence();
            self.destroy_block_in_creative(pos);
            self.joined.session.send_player_action_at(
                PlayerAction::StartDestroyBlock,
                pos,
                face,
                sequence,
            );
            return;
        }
        if !self.same_destroy_target(pos) {
            self.start_destroy(pos, face);
            return;
        }
        if self.is_air(pos) {
            self.interaction.destroying = None;
            return;
        }
        let progress = self.miner().destroy_progress(self.block_at(pos));
        let (face_changed, finished, hit) = {
            let Some(target) = self.interaction.destroying.as_mut() else {
                return;
            };
            target.progress += progress;
            target.ticks += 1;
            let face_changed = target.face != face;
            if face_changed {
                target.face = face;
            }
            (face_changed, target.progress >= 1.0, target.ticks % 4 == 0)
        };
        if hit && !finished {
            let state = self.block_at(pos);
            self.queue_block_sound(state, BlockSound::Hit, sounds::block_center(pos));
        }
        if !finished {
            let side = crate::world::block::direction::Direction::ALL[usize::from(face.min(5))];
            self.mining_particles(pos, side);
        }
        if finished {
            let sequence = self.next_sequence();
            self.destroy_block(pos);
            self.joined.session.send_player_action_at(
                PlayerAction::StopDestroyBlock,
                pos,
                face,
                sequence,
            );
            self.interaction.destroying = None;
            self.interaction.destroy_delay = DESTROY_DELAY;
        } else if face_changed {
            self.joined.session.send_player_action_at(
                PlayerAction::ChangeDestroyDirection,
                pos,
                face,
                0,
            );
        }
    }

    /// Melon's block swap: attack and use land on the same block this tick
    /// (clicked, or held and due). The block breaks and the held one takes
    /// its spot in one step (`melon:swap`), so what rests on it stays. Only
    /// for instant breaks (creative, instant mining) and a server that
    /// registered the channel; elsewhere both run as usual.
    fn try_swap(&mut self) -> bool {
        let attack = self.keys.attack_clicked()
            || self.keys.attack
                && self.interaction.destroy_delay == 0
                && self.interaction.miss_time == 0;
        let use_due = self.keys.use_clicked()
            || self.keys.use_item && self.interaction.right_click_delay == 0;
        if !attack
            || !use_due
            || self.interaction.destroying.is_some()
            || self.game_mode.current.is_spectator()
            || self.hit_entity().is_some()
            || !self.server_channels.iter().any(|c| c == SWAP_CHANNEL)
        {
            return false;
        }
        let Some(hit) = self.hit_block().filter(|hit| !self.is_air(hit.pos)) else {
            return false;
        };
        let Some(hand) = Hand::ALL.into_iter().find(|&hand| {
            let stack = self.hand_stack(hand);
            !stack.is_empty() && Block::from_registry_key(stack.item.registry_key).is_some()
        }) else {
            return false;
        };
        let instant = if self.game_mode.current.has_infinite_materials() {
            breaks_blocks_in_creative(self.inventory.selected_stack())
        } else {
            self.miner().destroy_progress(self.block_at(hit.pos)) >= 1.0
        };
        if !instant {
            return false;
        }
        while self.keys.consume_attack() {}
        while self.keys.consume_use() {}
        let sequence = self.next_sequence();
        self.destroy_block(hit.pos);
        let placed = self.predict_place(hand, &hit);
        self.joined.session.send_swap_block(
            hand,
            hit.pos,
            hit.face,
            hit.cursor(),
            hit.inside,
            sequence,
        );
        self.swing(Hand::Main);
        self.joined.session.send_punch();
        if placed {
            self.swing(hand);
            self.hands.item_used(hand);
        }
        self.interaction.destroy_delay = DESTROY_DELAY;
        self.interaction.right_click_delay = RIGHT_CLICK_DELAY;
        true
    }

    /// `MultiPlayerGameMode.sameDestroyTarget`: still the same block with
    fn same_destroy_target(&self, pos: [i32; 3]) -> bool {
        self.interaction.destroying.as_ref().is_some_and(|target| {
            target.pos == pos
                && target
                    .item
                    .are_items_and_components_equal(self.inventory.selected_stack())
        })
    }

    fn destroy_block(&mut self, pos: [i32; 3]) {
        let state = self.block_at(pos);
        if !sounds::breaks_silently(state) {
            self.queue_block_sound(state, BlockSound::Break, sounds::block_center(pos));
            self.own_break_particles(pos, state);
        }
        self.predict_block(pos, 0);
    }

    /// `Minecraft.startUseItem`: each hand in turn, on the block looked
    fn start_use_item(&mut self) {
        if self.interaction.destroying.is_some() {
            return;
        }
        self.interaction.right_click_delay = RIGHT_CLICK_DELAY;
        let hit = self.hit_block();
        let entity = self.hit_entity();
        if self.game_mode.current.is_spectator() {
            if let Some(entity) = entity {
                self.interact(entity, Hand::Main);
                return;
            }
            if let Some(hit) = hit {
                let sequence = self.next_sequence();
                self.joined.session.send_use_item_on(
                    Hand::Main,
                    hit.pos,
                    hit.face,
                    hit.cursor(),
                    hit.inside,
                    sequence,
                );
            }
            return;
        }
        for hand in Hand::ALL {
            if let Some(entity) = entity {
                match self.interact(entity, hand) {
                    EntityUse::Pass => {}
                    EntityUse::Swing => {
                        self.swing(hand);
                        return;
                    }
                    EntityUse::Quiet => return,
                }
            }
            if let Some(hit) = &hit {
                let sequence = self.next_sequence();
                self.joined.session.send_use_item_on(
                    hand,
                    hit.pos,
                    hit.face,
                    hit.cursor(),
                    hit.inside,
                    sequence,
                );
                if let UseOutcome::Done { swing, used } = self.use_item_on(hand, hit) {
                    if swing {
                        self.swing(hand);
                        if used {
                            self.hands.item_used(hand);
                        }
                    }
                    return;
                }
            }
            if self.hand_stack(hand).is_empty() {
                continue;
            }
            let sequence = self.next_sequence();
            let yaw = super::minecraft_yaw(self.player.yaw());
            let pitch = self.player.pitch();
            self.joined
                .session
                .send_use_item(hand, sequence, yaw, pitch);
            if let UseOutcome::Done { swing, .. } = self.use_item(hand, hit.is_some()) {
                if swing {
                    self.swing(hand);
                }
                self.hands.item_used(hand);
                return;
            }
        }
    }

    /// `MultiPlayerGameMode.interact`: tells the server about a right
    fn interact(&mut self, hit: EntityHit, hand: Hand) -> EntityUse {
        let Some(entity) = self.entities.get(hit.id) else {
            return EntityUse::Pass;
        };
        let location = (hit.location - entity.tick_pos()).to_array();
        let own = self.joined.session.info.entity_id;
        let leashed_to = entity.leash_holder.and_then(|holder| {
            if holder == own {
                return Some(Holder::Me);
            }
            let holder = self.entities.get(holder)?;
            Some(if holder.kind == EntityKind::Player {
                Holder::Player
            } else {
                Holder::Other
            })
        });
        let target = entity_interact::Target {
            kind: entity.kind,
            appearance: &entity.appearance,
            saddled: entity.equipment.saddled,
            leashed_to,
            holds_leash: self.entities.holds_leash(hit.id),
            occupied: !entity.passengers.is_empty(),
            container: entity.ty.resource_name.contains("chest")
                || entity.ty.resource_name.contains("hopper"),
            response: entity.interaction.is_some_and(|b| b.response),
        };
        let sneaking = self.keys.sneak;
        self.joined
            .session
            .send_interact(hit.id, hand, location, sneaking);
        if self.game_mode.current.is_spectator() {
            return EntityUse::Pass;
        }
        entity_interact::predict(target, self.hand_stack(hand), sneaking)
    }

    /// `MultiPlayerGameMode.performUseItemOn`: the block's own use (only
    fn use_item_on(&mut self, hand: Hand, hit: &BlockHit) -> UseOutcome {
        let clicked = self.block_at(hit.pos);
        let holding_any =
            !self.hand_stack(Hand::Main).is_empty() || !self.hand_stack(Hand::Off).is_empty();
        let suppress_block = self.keys.sneak && holding_any;
        if !suppress_block && hand == Hand::Main && is_interactive_block(clicked) {
            return UseOutcome::Done {
                swing: true,
                used: false,
            };
        }
        let stack = self.hand_stack(hand);
        if stack.is_empty() {
            return UseOutcome::Pass;
        }
        if Block::from_registry_key(stack.item.registry_key).is_some() {
            let placed = self.predict_place(hand, hit);
            return UseOutcome::Done {
                swing: placed,
                used: placed,
            };
        }
        let above = self.block_at([hit.pos[0], hit.pos[1] + 1, hit.pos[2]]);
        if acts_on_block(
            stack.item.registry_key,
            block_name(clicked),
            is_air_state(above),
        ) {
            UseOutcome::Done {
                swing: true,
                used: false,
            }
        } else {
            UseOutcome::Pass
        }
    }

    /// `Item.use`: food and shields are held up while use is held, bows and
    fn use_item(&mut self, hand: Hand, at_block: bool) -> UseOutcome {
        let stack = self.hand_stack(hand).clone();
        let key = stack.item.registry_key;
        if let Some(item_use) = ItemUse::of(&stack) {
            let creative = self.game_mode.current.has_infinite_materials();
            let ready = ItemUse::can_start(&stack, self.status.food)
                && (key != "bow" || creative || self.has_arrows());
            if !ready {
                return UseOutcome::Pass;
            }
            self.hands.using = Some(UsingItem {
                hand,
                item_use,
                remaining: item_use.duration,
                overrun: 0,
            });
            return UseOutcome::Done {
                swing: false,
                used: true,
            };
        }
        if is_thrown(key) || at_block && is_placed_from_bucket(key) {
            UseOutcome::Done {
                swing: true,
                used: true,
            }
        } else {
            UseOutcome::Pass
        }
    }

    /// Arrows anywhere in the inventory, for a bow to draw.
    fn has_arrows(&self) -> bool {
        self.inventory
            .slots()
            .iter()
            .any(|stack| !stack.is_empty() && stack.item.registry_key.ends_with("arrow"))
    }

    /// Places `hand`'s block where the click would, as the server will;
    fn predict_place(&mut self, hand: Hand, hit: &BlockHit) -> bool {
        let held = self.hand_stack(hand);
        let Some(block) = Block::from_registry_key(held.item.registry_key) else {
            return false;
        };
        let Some(face) = block_placement::Dir::from_face(hit.face) else {
            return false;
        };
        let blocks = |p: [i32; 3]| self.block_at(p);
        let input = block_placement::PlaceInput {
            blocks: &blocks,
            hit_pos: hit.pos,
            face,
            location: hit.location,
            yaw: super::minecraft_yaw(self.player.yaw()),
            pitch: self.player.pitch(),
            sneaking: self.keys.sneak,
        };
        let Some(placement) = block_placement::place(block, &input) else {
            return false;
        };
        let (target, state) = (placement.pos, placement.state);
        if self.placement_obstructed(target, state) {
            return false;
        }
        self.predict_block(target, state);
        for &(pos, state) in &placement.also {
            self.predict_block(pos, state);
        }
        self.queue_block_sound(state, BlockSound::Place, sounds::block_center(target));
        if !self.game_mode.current.has_infinite_materials() {
            let slot = match hand {
                Hand::Main => self.inventory.selected_slot(),
                Hand::Off => OFFHAND_SLOT,
            };
            let stack = &mut self.inventory.slots_mut()[slot];
            if stack.item_count <= 1 {
                *stack = empty();
            } else {
                stack.item_count -= 1;
            }
        }
        true
    }

    /// `Level.isUnobstructed`: a block can't go where a body that blocks
    fn placement_obstructed(&self, pos: [i32; 3], state: u16) -> bool {
        let Some(id) = BlockStateId::new(state) else {
            return false;
        };
        let origin = glam::DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
        let shapes: Vec<crate::world::physics::Aabb> = id
            .to_state()
            .get_block_collision_shapes()
            .map(|b| crate::world::physics::Aabb {
                min: origin + glam::DVec3::new(b.min.x, b.min.y, b.min.z),
                max: origin + glam::DVec3::new(b.max.x, b.max.y, b.max.z),
            })
            .collect();
        if shapes.is_empty() {
            return false;
        }
        if !self.game_mode.current.is_spectator() {
            let own = self.player.collision_box();
            if shapes.iter().any(|s| s.intersects(&own)) {
                return true;
            }
        }
        self.entities.iter().any(|e| {
            if e.id == self.entities.own_id() || !e.blocks_building() {
                return false;
            }
            let (w, h) = e.size();
            let bounds =
                crate::world::physics::Aabb::at_feet(e.pos(1.0), f64::from(w), f64::from(h));
            shapes.iter().any(|s| s.intersects(&bounds))
        })
    }

    fn pick_block(&mut self) {
        let Some(hit) = self.hit_block() else {
            return;
        };
        self.joined
            .session
            .send_pick_item_from_block(hit.pos, self.keys.sprint);
    }

    fn is_air(&self, pos: [i32; 3]) -> bool {
        is_air_state(self.block_at(pos))
    }

    fn block_at(&self, [x, y, z]: [i32; 3]) -> u16 {
        self.level.block_state(x, y, z).unwrap_or(0)
    }

    /// A click in a server-opened menu, or the player's own (`container` 0).
    pub fn click_menu(&mut self, container: i32, slot: i16, button: i8, kind: ClickType) {
        if container == 0 {
            self.click_inventory(slot, button, kind);
            return;
        }
        let creative = self.game_mode.current.has_infinite_materials();
        let click = {
            let Some(menu) = self.menus.open_container.as_mut() else {
                return;
            };
            if menu.id != container {
                return;
            }
            menu.click(slot, button, kind, creative, self.inventory.carried_mut())
        };
        if let Some(menu) = &self.menus.open_container {
            menu.copy_player_into(&mut self.inventory);
        }
        let Some(menu) = &self.menus.open_container else {
            return;
        };
        let changed: Vec<_> = click
            .changed
            .iter()
            .filter_map(|&i| {
                usize::try_from(i)
                    .ok()
                    .and_then(|index| menu.slots.get(index))
                    .map(|stack| (i, stack))
            })
            .collect();
        self.joined.session.send_container_click(
            container,
            menu.state_id,
            click.slot,
            click.button,
            click.kind,
            &changed,
            self.inventory.carried(),
        );
    }
}

fn is_air_state(state: u16) -> bool {
    state == 0 || BlockStateId::new(state).is_some_and(|id| id.to_state().is_air())
}

fn block_name(state: u16) -> &'static str {
    BlockStateId::new(state).map_or("air", |id| Block::from_state_id(id).name)
}

/// Blocks whose `useWithoutItem` does something with an empty hand: menus,
fn is_interactive_block(state: u16) -> bool {
    let name = block_name(state);
    is_menu_block(name)
        || (name.ends_with("_door") || name.ends_with("_trapdoor")) && !name.starts_with("iron_")
        || name.ends_with("_fence_gate")
        || name.ends_with("_button")
        || matches!(
            name,
            "lever" | "repeater" | "comparator" | "note_block" | "bell" | "daylight_detector"
        )
}

/// Blocks whose `useWithoutItem` opens a menu.
fn is_menu_block(name: &str) -> bool {
    matches!(
        name,
        "chest"
            | "trapped_chest"
            | "ender_chest"
            | "barrel"
            | "crafting_table"
            | "furnace"
            | "blast_furnace"
            | "smoker"
            | "enchanting_table"
            | "anvil"
            | "chipped_anvil"
            | "damaged_anvil"
            | "grindstone"
            | "cartography_table"
            | "loom"
            | "smithing_table"
            | "stonecutter"
            | "hopper"
            | "dispenser"
            | "dropper"
            | "brewing_stand"
            | "beacon"
            | "lectern"
            | "crafter"
            | "shulker_box"
    ) || name.ends_with("_shulker_box")
}

/// `Item.useOn` of the items that do something to the block clicked
fn acts_on_block(item: &str, clicked: &str, above_air: bool) -> bool {
    if item.ends_with("_spawn_egg") {
        return true;
    }
    if item.ends_with("_axe") {
        let strippable = ["_log", "_wood", "_stem", "_hyphae"]
            .iter()
            .any(|end| clicked.ends_with(end))
            && !clicked.starts_with("stripped_");
        let copper = clicked.contains("copper")
            && (clicked.starts_with("waxed_")
                || clicked.contains("exposed")
                || clicked.contains("weathered")
                || clicked.contains("oxidized"));
        return strippable || copper || clicked == "bamboo_block";
    }
    if item.ends_with("_shovel") {
        let path = matches!(
            clicked,
            "grass_block" | "dirt" | "podzol" | "coarse_dirt" | "mycelium" | "rooted_dirt"
        );
        return path && above_air || clicked.ends_with("campfire");
    }
    if item.ends_with("_hoe") {
        let tilled = matches!(
            clicked,
            "grass_block" | "dirt_path" | "dirt" | "coarse_dirt" | "rooted_dirt"
        );
        return tilled && above_air;
    }
    matches!(
        item,
        "flint_and_steel"
            | "fire_charge"
            | "armor_stand"
            | "item_frame"
            | "glow_item_frame"
            | "painting"
            | "end_crystal"
    ) || item == "bone_meal" && is_growable(clicked)
}

/// What bone meal grows.
fn is_growable(name: &str) -> bool {
    name.ends_with("_sapling")
        || name.ends_with("_mushroom")
        || name.ends_with("_stem")
        || name.ends_with("_nylium")
        || matches!(
            name,
            "grass_block"
                | "moss_block"
                | "wheat"
                | "carrots"
                | "potatoes"
                | "beetroots"
                | "short_grass"
                | "fern"
                | "bamboo"
                | "bamboo_sapling"
                | "sweet_berry_bush"
                | "cocoa"
                | "kelp"
                | "sea_pickle"
                | "seagrass"
                | "azalea"
                | "flowering_azalea"
                | "cave_vines"
                | "cave_vines_plant"
                | "glow_lichen"
                | "mangrove_propagule"
                | "pink_petals"
                | "big_dripleaf"
                | "small_dripleaf"
                | "torchflower_crop"
                | "pitcher_crop"
                | "rooted_dirt"
        )
}

/// Items `use` throws (`ProjectileItem`s and the fishing rod).
fn is_thrown(item: &str) -> bool {
    matches!(
        item,
        "snowball"
            | "egg"
            | "blue_egg"
            | "brown_egg"
            | "ender_pearl"
            | "experience_bottle"
            | "splash_potion"
            | "lingering_potion"
            | "wind_charge"
            | "fishing_rod"
    )
}

/// Buckets `use` empties onto the block looked at.
fn is_placed_from_bucket(item: &str) -> bool {
    item.ends_with("_bucket") && item != "milk_bucket"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str) -> u16 {
        Block::from_registry_key(name)
            .unwrap()
            .default_state
            .id
            .as_u16()
    }

    #[test]
    fn chests_doors_and_levers_are_used_by_hand() {
        assert!(is_interactive_block(state("chest")));
        assert!(is_interactive_block(state("oak_door")));
        assert!(is_interactive_block(state("lever")));
        assert!(!is_interactive_block(state("iron_door")));
        assert!(!is_interactive_block(state("stone")));
    }

    #[test]
    fn a_pickaxe_does_nothing_so_the_offhand_gets_the_click() {
        assert!(!acts_on_block("diamond_pickaxe", "stone", true));
        assert!(!acts_on_block("diamond_axe", "stone", true));
        assert!(acts_on_block("diamond_axe", "oak_log", true));
        assert!(!acts_on_block("diamond_axe", "stripped_oak_log", true));
        assert!(acts_on_block("iron_shovel", "grass_block", true));
        assert!(!acts_on_block("iron_shovel", "grass_block", false));
        assert!(acts_on_block("bone_meal", "oak_sapling", true));
        assert!(!acts_on_block("bone_meal", "stone", true));
    }
}
