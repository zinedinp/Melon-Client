use std::collections::HashMap;

use glam::DVec3;
use pumpkin_data::Block;

use super::*;

/// The state of `block` with all of `props` (`key`, `value`).
fn state(block: &Block, props: &[(&str, &str)]) -> u16 {
    block
        .states
        .iter()
        .find(|s| {
            let all = block
                .properties(s.id)
                .map(|p| p.to_props())
                .unwrap_or_default();
            props
                .iter()
                .all(|(k, v)| all.iter().any(|q| q.key_str() == *k && q.value_str() == *v))
        })
        .map(|s| s.id.as_u16())
        .expect("state")
}

fn prop(state: u16, key: &str) -> String {
    let id = BlockStateId::new_or_air(state);
    Block::from_state_id(id)
        .properties(id)
        .and_then(|p| {
            p.to_props()
                .into_iter()
                .find(|q| q.key_str() == key)
                .map(|q| q.value_str().to_owned())
        })
        .unwrap_or_default()
}

struct World(HashMap<[i32; 3], u16>);

impl World {
    /// Stone below y 0.
    fn new() -> World {
        let mut blocks = HashMap::new();
        for x in -3..=3 {
            for z in -3..=3 {
                blocks.insert([x, -1, z], Block::STONE.default_state.id.as_u16());
            }
        }
        World(blocks)
    }

    fn put(&mut self, pos: [i32; 3], state: u16) -> &mut World {
        self.0.insert(pos, state);
        self
    }

    /// Clicks `face` of `hit` at `location` holding `block`.
    fn click(
        &self,
        block: &'static Block,
        hit: [i32; 3],
        face: Dir,
        location: DVec3,
        (yaw, pitch): (f32, f32),
        sneaking: bool,
    ) -> Option<Placement> {
        let get = |p: [i32; 3]| self.0.get(&p).copied().unwrap_or(0);
        place(
            block,
            &PlaceInput {
                blocks: &get,
                hit_pos: hit,
                face,
                location,
                yaw,
                pitch,
                sneaking,
            },
        )
    }

    /// Clicks the top of the floor block below `pos`.
    fn on_floor(
        &self,
        block: &'static Block,
        pos: [i32; 3],
        look: (f32, f32),
    ) -> Option<Placement> {
        let [x, y, z] = pos;
        let hit = [x, y - 1, z];
        let at = DVec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5);
        self.click(block, hit, Dir::Up, at, look, false)
    }
}

/// Yaw 0 looks south, 180 north (vanilla degrees).
const LOOK_SOUTH: (f32, f32) = (0.0, 30.0);
const LOOK_NORTH: (f32, f32) = (180.0, 30.0);

#[test]
fn a_furnace_faces_the_player_and_an_anvil_turns_across() {
    let world = World::new();
    let furnace = world
        .on_floor(&Block::FURNACE, [0, 0, 0], LOOK_SOUTH)
        .unwrap();
    assert_eq!(furnace.pos, [0, 0, 0]);
    assert_eq!(prop(furnace.state, "facing"), "north");
    let anvil = world
        .on_floor(&Block::ANVIL, [0, 0, 0], LOOK_SOUTH)
        .unwrap();
    assert_eq!(prop(anvil.state, "facing"), "west");
}

#[test]
fn a_piston_faces_back_along_the_look_and_an_observer_along_it() {
    let world = World::new();
    let down = (0.0, 80.0);
    let piston = world.on_floor(&Block::PISTON, [0, 0, 0], down).unwrap();
    assert_eq!(prop(piston.state, "facing"), "up");
    let observer = world.on_floor(&Block::OBSERVER, [0, 0, 0], down).unwrap();
    assert_eq!(prop(observer.state, "facing"), "down");
}

#[test]
fn a_torch_clicked_on_a_wall_hangs_on_it_and_stands_on_the_floor() {
    let mut world = World::new();
    world.put([1, 0, 0], Block::STONE.default_state.id.as_u16());
    let side = world
        .click(
            &Block::TORCH,
            [1, 0, 0],
            Dir::West,
            DVec3::new(1.0, 0.5, 0.5),
            (-90.0, 0.0),
            false,
        )
        .unwrap();
    assert_eq!(side.pos, [0, 0, 0]);
    assert_eq!(
        Block::from_state_id(BlockStateId::new_or_air(side.state)),
        &Block::WALL_TORCH
    );
    assert_eq!(prop(side.state, "facing"), "west");
    let floor = world
        .on_floor(&Block::TORCH, [0, 0, 2], LOOK_SOUTH)
        .unwrap();
    assert_eq!(floor.state, Block::TORCH.default_state.id.as_u16());
}

#[test]
fn a_torch_with_nothing_to_hold_on_to_is_not_placed() {
    let mut world = World::new();
    world.put([0, 4, 0], Block::GLASS.default_state.id.as_u16());
    // Under the glass, in the air: nothing to stand on or hang from.
    let placed = world.click(
        &Block::TORCH,
        [0, 4, 0],
        Dir::Down,
        DVec3::new(0.5, 4.0, 0.5),
        (0.0, -80.0),
        false,
    );
    assert_eq!(placed, None);
}

#[test]
fn stairs_turn_upside_down_from_a_high_click_and_round_a_corner() {
    let mut world = World::new();
    world.put([1, 0, 0], Block::STONE.default_state.id.as_u16());
    let high = world
        .click(
            &Block::OAK_STAIRS,
            [1, 0, 0],
            Dir::West,
            DVec3::new(1.0, 0.8, 0.5),
            (90.0, 0.0),
            false,
        )
        .unwrap();
    assert_eq!(prop(high.state, "half"), "top");
    assert_eq!(prop(high.state, "facing"), "west");

    let east = state(
        &Block::OAK_STAIRS,
        &[
            ("facing", "east"),
            ("half", "bottom"),
            ("shape", "straight"),
        ],
    );
    let mut world = World::new();
    world.put([0, 0, -1], east);
    // Facing north with stairs facing east behind: an outer corner.
    let corner = world
        .on_floor(&Block::OAK_STAIRS, [0, 0, 0], LOOK_NORTH)
        .unwrap();
    assert_eq!(prop(corner.state, "facing"), "north");
    assert_eq!(prop(corner.state, "shape"), "outer_right");
}

#[test]
fn a_slab_on_a_slab_makes_a_double_one() {
    let mut world = World::new();
    let bottom = state(
        &Block::OAK_SLAB,
        &[("type", "bottom"), ("waterlogged", "false")],
    );
    world.put([0, 0, 0], bottom);
    let double = world
        .click(
            &Block::OAK_SLAB,
            [0, 0, 0],
            Dir::Up,
            DVec3::new(0.5, 0.5, 0.5),
            LOOK_SOUTH,
            false,
        )
        .unwrap();
    assert_eq!(double.pos, [0, 0, 0]);
    assert_eq!(prop(double.state, "type"), "double");
    let side_high = world
        .click(
            &Block::OAK_SLAB,
            [0, -1, 1],
            Dir::Up,
            DVec3::new(0.5, 0.0, 1.5),
            LOOK_SOUTH,
            false,
        )
        .unwrap();
    assert_eq!(prop(side_high.state, "type"), "bottom");
}

#[test]
fn a_chest_beside_another_joins_it_and_turns_it_into_the_other_half() {
    let mut world = World::new();
    let single = state(&Block::CHEST, &[("facing", "north"), ("type", "single")]);
    world.put([1, 0, 0], single);
    let placed = world
        .on_floor(&Block::CHEST, [0, 0, 0], LOOK_SOUTH)
        .unwrap();
    assert_eq!(prop(placed.state, "facing"), "north");
    assert_eq!(prop(placed.state, "type"), "left");
    assert_eq!(placed.also.len(), 1);
    assert_eq!(placed.also[0].0, [1, 0, 0]);
    assert_eq!(prop(placed.also[0].1, "type"), "right");

    let sneaking = world
        .click(
            &Block::CHEST,
            [0, -1, 0],
            Dir::Up,
            DVec3::new(0.5, 0.0, 0.5),
            LOOK_SOUTH,
            true,
        )
        .unwrap();
    assert_eq!(prop(sneaking.state, "type"), "single");
}

#[test]
fn a_trapped_chest_does_not_join_a_wooden_one() {
    let mut world = World::new();
    let single = state(&Block::CHEST, &[("facing", "north"), ("type", "single")]);
    world.put([1, 0, 0], single);
    let placed = world
        .on_floor(&Block::TRAPPED_CHEST, [0, 0, 0], LOOK_SOUTH)
        .unwrap();
    assert_eq!(prop(placed.state, "type"), "single");
}

#[test]
fn a_door_brings_its_upper_half() {
    let world = World::new();
    let door = world
        .on_floor(&Block::OAK_DOOR, [0, 0, 0], LOOK_SOUTH)
        .unwrap();
    assert_eq!(prop(door.state, "half"), "lower");
    assert_eq!(prop(door.state, "facing"), "south");
    assert_eq!(door.also.len(), 1);
    assert_eq!(door.also[0].0, [0, 1, 0]);
    assert_eq!(prop(door.also[0].1, "half"), "upper");
}

#[test]
fn a_log_lies_along_the_clicked_face_and_a_lever_hangs_on_a_wall() {
    let mut world = World::new();
    world.put([1, 0, 0], Block::STONE.default_state.id.as_u16());
    let side = DVec3::new(1.0, 0.5, 0.5);
    let log = world
        .click(
            &Block::OAK_LOG,
            [1, 0, 0],
            Dir::West,
            side,
            (-90.0, 0.0),
            false,
        )
        .unwrap();
    assert_eq!(prop(log.state, "axis"), "x");
    let lever = world
        .click(
            &Block::LEVER,
            [1, 0, 0],
            Dir::West,
            side,
            (-90.0, 0.0),
            false,
        )
        .unwrap();
    assert_eq!(prop(lever.state, "face"), "wall");
    assert_eq!(prop(lever.state, "facing"), "west");
}

#[test]
fn a_sign_turns_toward_the_player_and_a_lantern_hangs_from_a_ceiling() {
    let mut world = World::new();
    let sign = world
        .on_floor(&Block::OAK_SIGN, [0, 0, 0], LOOK_SOUTH)
        .unwrap();
    assert_eq!(prop(sign.state, "rotation"), "8");
    world.put([0, 3, 0], Block::STONE.default_state.id.as_u16());
    let lantern = world
        .click(
            &Block::LANTERN,
            [0, 3, 0],
            Dir::Down,
            DVec3::new(0.5, 3.0, 0.5),
            (0.0, -80.0),
            false,
        )
        .unwrap();
    assert_eq!(lantern.pos, [0, 2, 0]);
    assert_eq!(prop(lantern.state, "hanging"), "true");
}

#[test]
fn a_block_placed_in_water_is_waterlogged() {
    let mut world = World::new();
    world.put([0, 0, 0], Block::WATER.default_state.id.as_u16());
    let slab = world
        .click(
            &Block::OAK_SLAB,
            [0, -1, 0],
            Dir::Up,
            DVec3::new(0.5, 0.0, 0.5),
            LOOK_SOUTH,
            false,
        )
        .unwrap();
    assert_eq!(slab.pos, [0, 0, 0]);
    assert_eq!(prop(slab.state, "waterlogged"), "true");
}
