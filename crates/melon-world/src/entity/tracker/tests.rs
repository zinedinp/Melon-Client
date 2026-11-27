use super::*;
use crate::entity::wire::DataValue;

fn add(id: i32, kind: &EntityType, pos: [f64; 3]) -> EntityPacket {
    EntityPacket::Add(AddEntity {
        id,
        uuid: uuid::Uuid::nil(),
        kind: i32::from(kind.id),
        pos,
        velocity: [0.0; 3],
        pitch: 0.0,
        yaw: 0.0,
        head_yaw: 0.0,
        data: 0,
    })
}

#[test]
fn a_relative_move_glides_there_in_three_ticks() {
    let mut entities = Entities::default();
    entities.apply(add(5, &EntityType::PIG, [0.5, 64.0, 0.5]), 1);
    entities.apply(
        EntityPacket::Move(MoveEntity {
            id: 5,
            steps: vec![[4096, 0, 0]],
            rotation: None,
            on_ground: true,
        }),
        1,
    );
    entities.tick(true);
    let pig = entities.get(5).unwrap();
    assert!((pig.pos(1.0).x - (0.5 + 1.0 / 3.0)).abs() < 1e-9);
    entities.tick(true);
    entities.tick(true);
    let pig = entities.get(5).unwrap();
    assert!((pig.pos(1.0).x - 1.5).abs() < 1e-9);
    assert!(pig.walk.speed(1.0) > 0.0);
    entities.apply(EntityPacket::Remove(vec![5]), 1);
    assert!(entities.is_empty());
}

#[test]
fn steps_chain_from_the_base_not_the_drawn_position() {
    let mut entities = Entities::default();
    entities.apply(add(2, &EntityType::ZOMBIE, [0.0, 0.0, 0.0]), 1);
    for _ in 0..2 {
        entities.apply(
            EntityPacket::Move(MoveEntity {
                id: 2,
                steps: vec![[2048, 0, 0], [2048, 0, 0]],
                rotation: None,
                on_ground: true,
            }),
            1,
        );
    }
    for _ in 0..3 {
        entities.tick(true);
    }
    assert!((entities.get(2).unwrap().pos(1.0).x - 2.0).abs() < 1e-9);
}

#[test]
fn a_slime_is_as_big_as_its_size_and_squashes_when_it_lands() {
    let mut entities = Entities::default();
    entities.apply(add(6, &EntityType::SLIME, [0.0, 64.0, 0.0]), 1);
    entities.apply(
        EntityPacket::Data {
            id: 6,
            values: vec![(18, DataValue::Int(4))],
        },
        1,
    );
    let slime = entities.get(6).unwrap();
    assert!((slime.size().0 - 4.0 * 0.52).abs() < 1e-5);
    entities.by_id.get_mut(&6).unwrap().on_ground = false;
    entities.tick(true);
    entities.by_id.get_mut(&6).unwrap().on_ground = true;
    for _ in 0..2 {
        entities.tick(true);
    }
    let squished = entities.get(6).unwrap().squish(1.0);
    assert!(squished < 0.0, "{squished}");
    for _ in 0..40 {
        entities.tick(true);
    }
    assert!(entities.get(6).unwrap().squish(1.0).abs() < 1e-3);
}

#[test]
fn an_iron_golem_swings_its_arms_for_ten_ticks_after_an_attack() {
    let mut entities = Entities::default();
    entities.apply(add(8, &EntityType::IRON_GOLEM, [0.0; 3]), 1);
    assert_eq!(entities.get(8).unwrap().golem_pose(0.0).attack, 0.0);
    entities.apply(
        EntityPacket::Event {
            id: 8,
            event: golem::EVENT_ATTACK,
        },
        1,
    );
    assert_eq!(entities.get(8).unwrap().golem_pose(0.5).attack, 9.5);
    for _ in 0..golem::ATTACK_TICKS {
        entities.tick(true);
    }
    assert_eq!(entities.get(8).unwrap().golem_pose(0.5).attack, 0.0);
    entities.apply(
        EntityPacket::Event {
            id: 8,
            event: golem::EVENT_OFFER_FLOWER,
        },
        1,
    );
    assert_eq!(entities.get(8).unwrap().golem_pose(0.0).offer_flower, 400);
    entities.apply(
        EntityPacket::Event {
            id: 8,
            event: golem::EVENT_STOP_OFFER_FLOWER,
        },
        1,
    );
    assert_eq!(entities.get(8).unwrap().golem_pose(0.0).offer_flower, 0);
}

#[test]
fn a_mob_named_dinnerbone_hangs_upside_down_with_its_look_inverted() {
    let mut entities = Entities::default();
    entities.apply(add(5, &EntityType::COW, [0.0; 3]), 1);
    let name = |name: Option<&str>| EntityPacket::Data {
        id: 5,
        values: vec![(
            2,
            DataValue::Name(name.map(|n| Box::new(Component::text(n.to_owned())))),
        )],
    };
    let plain = |c: &Component| c.plain_text();
    assert_eq!(entities.get(5).unwrap().living_state(0.0).upside_down, None);
    entities.apply(name(Some("Dinnerbone")), 1);
    entities.resolve_names(&plain);
    let cow = entities.get(5).unwrap();
    assert!(cow.custom_name.is_some());
    let state = cow.living_state(0.0);
    assert_eq!(state.upside_down, Some(cow.size().1));
    entities.apply(name(Some("Cow")), 1);
    entities.resolve_names(&plain);
    assert_eq!(entities.get(5).unwrap().living_state(0.0).upside_down, None);
    entities.apply(name(Some("Grumm")), 1);
    entities.resolve_names(&plain);
    assert!(entities.get(5).unwrap().upside_down());
    entities.apply(name(None), 1);
    entities.resolve_names(&plain);
    let cow = entities.get(5).unwrap();
    assert!(cow.custom_name.is_none() && !cow.upside_down());
}

#[test]
fn a_name_that_translates_to_dinnerbone_does_what_dinnerbone_does() {
    use crate::i18n::Lang;
    use crate::text::StyledText;
    let upside_down_in = |lang: &Lang| {
        let mut entities = Entities::default();
        entities.apply(add(5, &EntityType::COW, [0.0; 3]), 1);
        let name = Component::translatable("my.name", Vec::new());
        entities.apply(
            EntityPacket::Data {
                id: 5,
                values: vec![(2, DataValue::Name(Some(Box::new(name))))],
            },
            1,
        );
        entities.resolve_names(&|c| StyledText::from_component(c, lang).to_plain());
        entities.get(5).unwrap().upside_down()
    };
    assert!(!upside_down_in(&Lang::from_pairs(&[])));
    assert!(upside_down_in(&Lang::from_pairs(&[(
        "my.name",
        "Dinnerbone"
    )])));
}

#[test]
fn switching_the_language_looks_at_the_names_again() {
    use crate::i18n::Lang;
    use crate::text::StyledText;
    let mut entities = Entities::default();
    entities.apply(add(5, &EntityType::COW, [0.0; 3]), 1);
    let name = Component::translatable("my.name", Vec::new());
    entities.apply(
        EntityPacket::Data {
            id: 5,
            values: vec![(2, DataValue::Name(Some(Box::new(name))))],
        },
        1,
    );
    let english = Lang::from_pairs(&[]);
    let german = Lang::from_pairs(&[("my.name", "Dinnerbone")]);
    entities.resolve_names(&|c| StyledText::from_component(c, &english).to_plain());
    assert!(!entities.get(5).unwrap().upside_down());
    entities.resolve_names(&|c| StyledText::from_component(c, &german).to_plain());
    assert!(!entities.get(5).unwrap().upside_down());
    entities.rename_all();
    entities.resolve_names(&|c| StyledText::from_component(c, &german).to_plain());
    assert!(entities.get(5).unwrap().upside_down());
}

#[test]
fn an_enderman_carries_the_block_its_data_names() {
    use pumpkin_data::tracked_data::enderman;
    let mut entities = Entities::default();
    entities.apply(add(6, &EntityType::ENDERMAN, [0.0; 3]), 1);
    assert!(!entities.get(6).unwrap().appearance.carrying());
    let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
    entities.apply(
        EntityPacket::Data {
            id: 6,
            values: vec![(
                enderman::DATA_CARRY_STATE.id.0,
                DataValue::Int(i32::from(stone)),
            )],
        },
        1,
    );
    let enderman = &entities.get(6).unwrap().appearance;
    assert!(enderman.carrying());
    assert_eq!(enderman.block_state, stone);
    entities.apply(
        EntityPacket::Data {
            id: 6,
            values: vec![(enderman::DATA_CARRY_STATE.id.0, DataValue::Int(0))],
        },
        1,
    );
    assert!(!entities.get(6).unwrap().appearance.carrying());
}

#[test]
fn a_falling_block_is_the_state_its_spawn_data_names() {
    let sand = pumpkin_data::Block::SAND.default_state.id.as_u16();
    let mut entities = Entities::default();
    let EntityPacket::Add(mut falling) = add(8, &EntityType::FALLING_BLOCK, [1.5, 70.0, 1.5])
    else {
        unreachable!()
    };
    falling.data = i32::from(sand);
    entities.apply(EntityPacket::Add(falling), 1);
    let block = entities.get(8).unwrap();
    assert_eq!(block.kind, EntityKind::FallingBlock);
    assert_eq!(block.appearance.block_state, sand);
    assert!(!block.kind.is_mob());
}

#[test]
fn piglins_shake_where_the_dimension_zombifies_them_unless_immune() {
    use pumpkin_data::tracked_data::abstract_piglin;
    let mut entities = Entities::default();
    entities.apply(add(9, &EntityType::PIGLIN, [0.0; 3]), 1);
    entities.set_piglins_zombify(false);
    entities.tick(true);
    assert!(!entities.get(9).unwrap().appearance.shaking());
    entities.set_piglins_zombify(true);
    entities.tick(true);
    assert!(entities.get(9).unwrap().appearance.shaking());
    entities.apply(
        EntityPacket::Data {
            id: 9,
            values: vec![(
                abstract_piglin::DATA_IMMUNE_TO_ZOMBIFICATION.id.0,
                DataValue::Bool(true),
            )],
        },
        1,
    );
    entities.tick(true);
    assert!(!entities.get(9).unwrap().appearance.shaking());
}

#[test]
fn a_skeleton_turning_into_a_stray_shakes() {
    use pumpkin_data::tracked_data::skeleton;
    let mut entities = Entities::default();
    entities.apply(add(10, &EntityType::SKELETON, [0.0; 3]), 1);
    assert!(!entities.get(10).unwrap().appearance.shaking());
    entities.apply(
        EntityPacket::Data {
            id: 10,
            values: vec![(
                skeleton::DATA_STRAY_CONVERSION_ID.id.0,
                DataValue::Bool(true),
            )],
        },
        1,
    );
    assert!(entities.get(10).unwrap().appearance.shaking());
}

#[test]
fn a_rabbit_hops_when_the_server_says_so_and_its_coat_comes_from_the_data() {
    let mut entities = Entities::default();
    entities.apply(add(4, &EntityType::RABBIT, [0.0; 3]), 1);
    entities.tick(true);
    assert!(!entities.get(4).unwrap().anim[mobs::rabbit::HOP].is_started());
    entities.apply(
        EntityPacket::Event {
            id: 4,
            event: mobs::rabbit::EVENT_JUMP,
        },
        1,
    );
    entities.tick(true);
    entities.tick(true);
    assert!(entities.get(4).unwrap().anim[mobs::rabbit::HOP].is_started());
    entities.apply(
        EntityPacket::Data {
            id: 4,
            values: vec![(18, DataValue::Int(99))],
        },
        1,
    );
    assert_eq!(entities.get(4).unwrap().appearance.rabbit_variant, 99);
}

#[test]
fn a_bats_animation_follows_whether_it_hangs() {
    let mut entities = Entities::default();
    entities.apply(add(9, &EntityType::BAT, [0.0; 3]), 1);
    entities.tick(true);
    entities.tick(true);
    let bat = entities.get(9).unwrap();
    assert!(bat.anim[mobs::bat::FLY].is_started());
    assert!(!bat.anim[mobs::bat::REST].is_started());
    entities.apply(
        EntityPacket::Data {
            id: 9,
            values: vec![(16, DataValue::Byte(mobs::bat::FLAG_RESTING))],
        },
        1,
    );
    entities.tick(true);
    let bat = entities.get(9).unwrap();
    assert!(!bat.anim[mobs::bat::FLY].is_started());
    assert!(bat.anim[mobs::bat::REST].is_started());
}

#[test]
fn the_own_entity_is_not_tracked() {
    let mut entities = Entities::default();
    entities.apply(add(1, &EntityType::PLAYER, [0.0; 3]), 1);
    assert!(entities.is_empty());
}

#[test]
fn a_rider_follows_its_mount_and_stops_walking() {
    let mut entities = Entities::default();
    entities.apply(add(1, &EntityType::PIG, [10.0, 64.0, 10.0]), 99);
    entities.apply(add(2, &EntityType::ZOMBIE, [0.0, 64.0, 0.0]), 99);
    entities.apply(
        EntityPacket::Passengers {
            id: 1,
            passengers: vec![2],
        },
        99,
    );
    entities.tick(true);
    let zombie = entities.get(2).unwrap();
    assert_eq!(zombie.vehicle, Some(1));
    let at = zombie.tick_pos();
    assert!(
        (at.x - 10.0).abs() < 1e-9 && (at.z - 10.0).abs() < 1e-9,
        "{at}"
    );
    assert!((at.y - 64.0 - (0.868_75 - 0.7)).abs() < 1e-6, "{at}");
    assert!(zombie.living_state(0.5).riding);
    assert_eq!(zombie.living_state(0.5).walk_speed, 0.0);
    entities.apply(
        EntityPacket::SetPosition(crate::entity::wire::SetPosition {
            id: 1,
            pos: [12.0, 64.0, 10.0],
            velocity: None,
            yaw: 0.0,
            pitch: 0.0,
            relative: 0,
            on_ground: true,
        }),
        99,
    );
    for _ in 0..4 {
        entities.tick(true);
    }
    assert!((entities.get(2).unwrap().tick_pos().x - 12.0).abs() < 1e-6);
    entities.apply(
        EntityPacket::Passengers {
            id: 1,
            passengers: vec![],
        },
        99,
    );
    assert_eq!(entities.get(2).unwrap().vehicle, None);
    assert!(!entities.get(2).unwrap().living_state(0.5).riding);
}

#[test]
fn the_local_player_is_named_in_the_list_but_not_tracked() {
    let mut entities = Entities::default();
    entities.apply(add(1, &EntityType::HORSE, [0.0, 64.0, 0.0]), 7);
    entities.apply(
        EntityPacket::Passengers {
            id: 1,
            passengers: vec![7],
        },
        7,
    );
    assert_eq!(entities.own_vehicle(), Some(1));
    let seat = entities.own_seat().unwrap();
    assert!((seat.y - 64.0 - (1.443_75 - 0.6)).abs() < 1e-6);
    assert_eq!(
        entities.own_steering(ItemStack::EMPTY, ItemStack::EMPTY),
        Some(ride::Steering::Carried)
    );
    entities.apply(
        EntityPacket::Equipment {
            id: 1,
            slots: vec![(
                EquipmentSlot::Saddle,
                ItemStack::new(1, &pumpkin_data::item::Item::SADDLE),
            )],
        },
        7,
    );
    assert_eq!(
        entities.own_steering(ItemStack::EMPTY, ItemStack::EMPTY),
        Some(ride::Steering::Horse)
    );
    entities.apply(EntityPacket::Remove(vec![1]), 7);
    assert_eq!(entities.own_vehicle(), None);
}

#[test]
fn a_second_passenger_does_not_steer() {
    let mut entities = Entities::default();
    entities.apply(add(1, &EntityType::OAK_BOAT, [0.0, 64.0, 0.0]), 7);
    entities.apply(add(2, &EntityType::COW, [0.0, 64.0, 0.0]), 7);
    entities.apply(
        EntityPacket::Passengers {
            id: 1,
            passengers: vec![2, 7],
        },
        7,
    );
    assert_eq!(
        entities.own_steering(ItemStack::EMPTY, ItemStack::EMPTY),
        Some(ride::Steering::Carried)
    );
}

#[test]
fn hurt_and_death_show_and_time_out() {
    let mut entities = Entities::default();
    entities.apply(add(3, &EntityType::COW, [0.0; 3]), 1);
    entities.apply(EntityPacket::Hurt { id: 3, yaw: 0.0 }, 1);
    assert!(entities.get(3).unwrap().living_state(0.0).hurt);
    for _ in 0..HURT_DURATION {
        entities.tick(true);
    }
    assert!(!entities.get(3).unwrap().living_state(0.0).hurt);
    entities.apply(
        EntityPacket::Data {
            id: 3,
            values: vec![(9, DataValue::Float(0.0))],
        },
        1,
    );
    for _ in 0..30 {
        entities.tick(true);
    }
    let state = entities.get(3).unwrap().living_state(0.0);
    assert_eq!(state.death_time, DEATH_DURATION as f32);
    assert!(state.hurt);
}

#[test]
fn a_mob_body_follows_its_head_when_it_stands() {
    let mut entities = Entities::default();
    entities.apply(add(4, &EntityType::SHEEP, [0.0; 3]), 1);
    entities.apply(EntityPacket::HeadRotation { id: 4, yaw: 100.0 }, 1);
    for _ in 0..3 {
        entities.tick(true);
    }
    let state = entities.get(4).unwrap().living_state(1.0);
    assert!(
        state.y_rot.abs() <= BodyRotation::MAX_HEAD + 1e-3,
        "{}",
        state.y_rot
    );
}

/// A level of air (one chunk) with a stone floor at `floor_y`, if any.
fn level_with_floor(floor_y: Option<i32>) -> Level {
    use crate::chunk::{Chunk, Section};
    let mut level = Level::default();
    level.insert(Chunk::new(
        0,
        0,
        (0..24).map(|_| Section::empty()).collect(),
    ));
    if let Some(y) = floor_y {
        let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
        for x in 0..16 {
            for z in 0..16 {
                level.set_block([x, y, z], stone, false);
            }
        }
    }
    level
}

#[test]
fn a_falling_block_falls_by_vanilla_gravity_and_drag() {
    let level = level_with_floor(None);
    let mut entities = Entities::default();
    entities.apply(add(8, &EntityType::FALLING_BLOCK, [8.0, 100.0, 8.0]), 1);
    let mut y = 100.0;
    let mut v = 0.0;
    for _ in 0..10 {
        entities.tick(true);
        entities.simulate(&level);
        v -= 0.04;
        y += v;
        v *= f64::from(0.98_f32);
    }
    let at = entities.get(8).unwrap().tick_pos();
    assert!((at.y - y).abs() < 1e-9, "{} vs {y}", at.y);
}

#[test]
fn a_falling_block_stops_on_the_floor_and_does_not_sink_in() {
    let level = level_with_floor(Some(60));
    let mut entities = Entities::default();
    entities.apply(add(8, &EntityType::FALLING_BLOCK, [8.0, 64.0, 8.0]), 1);
    for _ in 0..60 {
        entities.tick(true);
        entities.simulate(&level);
    }
    let block = entities.get(8).unwrap();
    assert!(
        (block.tick_pos().y - 61.0).abs() < 1e-6,
        "{}",
        block.tick_pos().y
    );
    assert!(block.on_ground);
}

#[test]
fn a_block_bounces_on_slime_but_not_on_stone() {
    for (block, bounces) in [
        (pumpkin_data::Block::SLIME_BLOCK, true),
        (pumpkin_data::Block::STONE, false),
    ] {
        let mut level = level_with_floor(None);
        for x in 0..16 {
            for z in 0..16 {
                level.set_block([x, 60, z], block.default_state.id.as_u16(), false);
            }
        }
        let mut entities = Entities::default();
        entities.apply(add(8, &EntityType::FALLING_BLOCK, [8.0, 70.0, 8.0]), 1);
        let mut went_up = false;
        let mut landed = false;
        for _ in 0..80 {
            entities.tick(true);
            entities.simulate(&level);
            let b = entities.get(8).unwrap();
            landed |= b.on_ground;
            went_up |= landed && b.velocity.y > 0.0;
        }
        assert_eq!(went_up, bounces, "{}", block.name);
    }
}

#[test]
fn the_server_corrects_a_falling_block_and_its_velocity() {
    let level = level_with_floor(None);
    let mut entities = Entities::default();
    entities.apply(add(8, &EntityType::FALLING_BLOCK, [8.0, 100.0, 8.0]), 1);
    for _ in 0..5 {
        entities.tick(true);
        entities.simulate(&level);
    }
    entities.apply(
        EntityPacket::SetPosition(SetPosition {
            id: 8,
            pos: [8.0, 90.0, 8.0],
            yaw: 0.0,
            pitch: 0.0,
            relative: 0,
            on_ground: false,
            velocity: Some([0.0, -0.5, 0.0]),
        }),
        1,
    );
    assert_eq!(entities.get(8).unwrap().tick_pos().y, 90.0);
    entities.tick(true);
    entities.simulate(&level);
    let expected = 90.0 + (-0.5 - 0.04);
    assert!((entities.get(8).unwrap().tick_pos().y - expected).abs() < 1e-9);
    entities.apply(
        EntityPacket::Motion {
            id: 8,
            velocity: [0.0, 0.0, 0.0],
        },
        1,
    );
    let before = entities.get(8).unwrap().tick_pos().y;
    entities.tick(true);
    entities.simulate(&level);
    assert!((entities.get(8).unwrap().tick_pos().y - (before - 0.04)).abs() < 1e-9);
}

#[test]
fn a_block_in_an_unloaded_chunk_is_not_simulated() {
    let level = Level::default();
    let mut entities = Entities::default();
    entities.apply(add(8, &EntityType::FALLING_BLOCK, [8.0, 100.0, 8.0]), 1);
    entities.tick(true);
    entities.simulate(&level);
    assert_eq!(entities.get(8).unwrap().tick_pos().y, 100.0);
}

#[test]
fn primed_tnt_counts_its_fuse_down_and_goes_out() {
    let level = level_with_floor(Some(60));
    let mut entities = Entities::default();
    let EntityPacket::Add(mut tnt) = add(9, &EntityType::TNT, [8.0, 62.0, 8.0]) else {
        unreachable!()
    };
    tnt.velocity = [0.0, 0.2, 0.0];
    entities.apply(EntityPacket::Add(tnt), 1);
    let state = entities.get(9).unwrap().appearance.block_state;
    assert_eq!(state, pumpkin_data::Block::TNT.default_state.id.as_u16());
    entities.apply(
        EntityPacket::Data {
            id: 9,
            values: vec![(8, DataValue::Int(3))],
        },
        1,
    );
    assert_eq!(entities.get(9).unwrap().appearance.fuse, 3);
    for tick in 1..=3 {
        entities.tick(true);
        entities.simulate(&level);
        assert_eq!(entities.get(9).is_some(), tick < 3, "tick {tick}");
    }
}

#[test]
fn a_landing_tnt_stops_on_the_floor() {
    let level = level_with_floor(Some(60));
    let mut entities = Entities::default();
    entities.apply(add(9, &EntityType::TNT, [8.0, 61.05, 8.0]), 1);
    entities.apply(
        EntityPacket::Motion {
            id: 9,
            velocity: [0.0, -0.6, 0.0],
        },
        1,
    );
    entities.tick(true);
    entities.simulate(&level);
    let tnt = entities.get(9).unwrap();
    assert!((tnt.tick_pos().y - 61.0).abs() < 1e-9);
    assert!(tnt.on_ground);
    assert_eq!(tnt.velocity.y, 0.0);
}

#[test]
fn a_boat_stops_a_player_and_a_grown_ghast_only_one_above_it() {
    let mut entities = Entities::default();
    entities.apply(add(1, &EntityType::OAK_BOAT, [0.0, 64.0, 0.0]), 7);
    entities.apply(add(2, &EntityType::HAPPY_GHAST, [10.0, 64.0, 0.0]), 7);
    let near = Aabb::at_feet(DVec3::new(5.0, 64.0, 0.0), 40.0, 10.0);
    assert_eq!(entities.colliders_near(&near, 64.0).len(), 1);
    let top = 64.0 + f64::from(entities.get(2).unwrap().size().1);
    assert_eq!(entities.colliders_near(&near, top).len(), 2);
    entities.set_own_vehicle_for_test(Some(1));
    assert_eq!(entities.colliders_near(&near, 64.0).len(), 0);
}

#[test]
fn a_steered_boat_is_moved_by_the_client_and_not_by_the_server() {
    use crate::chunk::{Chunk, Section};
    let mut level = Level::default();
    level.insert(Chunk::new(
        0,
        0,
        (0..24).map(|_| Section::empty()).collect(),
    ));
    let water = pumpkin_data::Block::WATER.default_state.id.as_u16();
    let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
    for x in 0..16 {
        for z in 0..16 {
            level.set_block([x, 60, z], stone, false);
            for y in 61..=63 {
                level.set_block([x, y, z], water, false);
            }
        }
    }
    let mut entities = Entities::default();
    entities.apply(add(1, &EntityType::OAK_BOAT, [8.0, 62.5, 8.0]), 99);
    entities.apply(
        EntityPacket::Passengers {
            id: 1,
            passengers: vec![99],
        },
        99,
    );
    let ahead = DriveInput::Boat(boat_sim::BoatInput {
        up: true,
        ..Default::default()
    });
    for _ in 0..30 {
        entities.tick(true);
        entities.drive(&level, Some(ahead));
    }
    let driven = entities.driven_vehicle().expect("steered");
    let pos = driven.pos;
    assert!(pos.z > 8.0 + 2.0, "{pos}");
    assert_eq!(driven.paddles, Some((true, true)), "both row ahead");
    entities.apply(
        EntityPacket::SetPosition(SetPosition {
            id: 1,
            pos: [8.0, 62.5, 8.0],
            yaw: 0.0,
            pitch: 0.0,
            relative: 0,
            velocity: None,
            on_ground: false,
        }),
        99,
    );
    assert_eq!(entities.driven_vehicle().unwrap().pos, pos);
    assert!(entities.snap_driven(DVec3::new(8.0, 62.5, 8.0), 0.0, 0.0));
    assert_eq!(
        entities.driven_vehicle().unwrap().pos,
        DVec3::new(8.0, 62.5, 8.0),
        "the server's correction holds"
    );
    for _ in 0..30 {
        entities.tick(true);
        entities.drive(&level, Some(ahead));
    }
    entities.drive(&level, None);
    assert!(entities.driven_vehicle().is_none());
    assert!(
        !entities.snap_driven(DVec3::ZERO, 0.0, 0.0),
        "nothing to snap"
    );
    entities.apply(
        EntityPacket::Passengers {
            id: 1,
            passengers: vec![],
        },
        99,
    );
    let at = entities.get(1).unwrap().tick_pos();
    assert!(at.z > 8.0 + 2.0, "{at}");
    entities.apply(
        EntityPacket::Move(MoveEntity {
            id: 1,
            steps: vec![[0, 0, 4096]],
            rotation: None,
            on_ground: false,
        }),
        99,
    );
    for _ in 0..3 {
        entities.tick(true);
    }
    let after = entities.get(1).unwrap().tick_pos();
    assert!(
        (after.z - at.z - 1.0).abs() < 2.0 / 4096.0,
        "{after} from {at}"
    );
    assert!((after.x - at.x).abs() < 1e-6);
}

#[test]
fn a_steered_horse_walks_on_the_client() {
    use crate::chunk::{Chunk, Section};
    let mut level = Level::default();
    level.insert(Chunk::new(
        0,
        0,
        (0..24).map(|_| Section::empty()).collect(),
    ));
    let stone = pumpkin_data::Block::STONE.default_state.id.as_u16();
    for x in 0..16 {
        for z in 0..16 {
            level.set_block([x, 63, z], stone, false);
        }
    }
    let mut entities = Entities::default();
    entities.apply(add(1, &EntityType::HORSE, [8.0, 64.0, 8.0]), 99);
    entities.apply(
        EntityPacket::Equipment {
            id: 1,
            slots: vec![(
                EquipmentSlot::Saddle,
                ItemStack::new(1, &pumpkin_data::item::Item::SADDLE),
            )],
        },
        99,
    );
    entities.apply(
        EntityPacket::Passengers {
            id: 1,
            passengers: vec![99],
        },
        99,
    );
    let ahead = DriveInput::Horse(horse_sim::HorseInput::from_keys(1.0, 0.0, 0.0, 0.0));
    let paddles = DriveInput::Boat(boat_sim::BoatInput::default());
    entities.drive(&level, Some(paddles));
    assert!(
        entities.driven_vehicle().is_none(),
        "a horse takes no paddles"
    );
    for _ in 0..20 {
        entities.tick(true);
        entities.drive(&level, Some(ahead));
    }
    let driven = entities.driven_vehicle().expect("steered");
    let pos = driven.pos;
    assert!(pos.z > 8.0 + 1.5, "{pos}");
    assert_eq!((driven.yaw, driven.paddles), (0.0, None));
    entities.apply(
        EntityPacket::SetPosition(SetPosition {
            id: 1,
            pos: [8.0, 64.0, 8.0],
            yaw: 90.0,
            pitch: 0.0,
            relative: 0,
            velocity: None,
            on_ground: true,
        }),
        99,
    );
    assert_eq!(entities.driven_vehicle().unwrap().pos, pos);
    entities.drive(&level, None);
    assert!(entities.driven_vehicle().is_none());
}

#[test]
fn passengers_wait_for_the_vehicle_and_a_late_rider_sits() {
    let mut entities = Entities::default();
    entities.apply(
        EntityPacket::Passengers {
            id: 1,
            passengers: vec![2],
        },
        99,
    );
    entities.apply(add(1, &EntityType::PIG, [10.0, 64.0, 10.0]), 99);
    entities.apply(add(2, &EntityType::ZOMBIE, [0.0, 64.0, 0.0]), 99);
    entities.tick(true);
    let zombie = entities.get(2).unwrap();
    assert_eq!(zombie.vehicle, Some(1));
    let at = zombie.tick_pos();
    assert!(
        (at.x - 10.0).abs() < 1e-9 && (at.z - 10.0).abs() < 1e-9,
        "{at}"
    );
}

fn player_box(x: f64, y: f64, z: f64) -> Aabb {
    Aabb::at_feet(DVec3::new(x, y, z), 0.6, 1.8)
}

#[test]
fn a_mob_in_the_players_box_pushes_it_away_and_one_out_of_reach_does_not() {
    let mut entities = Entities::default();
    entities.apply(add(5, &EntityType::PIG, [0.0, 64.0, 0.0]), 1);
    let push = entities.push_on_player(&player_box(0.4, 64.0, 0.0), |_, _| true);
    assert!(push.x > 0.0 && push.z == 0.0 && push.y == 0.0, "{push:?}");
    assert_eq!(
        entities.push_on_player(&player_box(5.0, 64.0, 0.0), |_, _| true),
        DVec3::ZERO
    );
}

#[test]
fn bats_and_armor_stands_do_not_push() {
    let mut entities = Entities::default();
    entities.apply(add(5, &EntityType::BAT, [0.0, 64.0, 0.0]), 1);
    entities.apply(add(6, &EntityType::ARMOR_STAND, [0.0, 64.0, 0.0]), 1);
    assert_eq!(
        entities.push_on_player(&player_box(0.4, 64.0, 0.0), |_, _| true),
        DVec3::ZERO
    );
}

#[test]
fn the_teams_rules_can_stop_a_push() {
    let mut entities = Entities::default();
    entities.apply(add(5, &EntityType::PIG, [0.0, 64.0, 0.0]), 1);
    assert_eq!(
        entities.push_on_player(&player_box(0.4, 64.0, 0.0), |_, _| false),
        DVec3::ZERO
    );
}

#[test]
fn the_own_vehicle_does_not_push() {
    let mut entities = Entities::default();
    entities.apply(add(5, &EntityType::PIG, [0.0, 64.0, 0.0]), 1);
    entities.set_own_vehicle_for_test(Some(5));
    assert_eq!(
        entities.push_on_player(&player_box(0.4, 64.0, 0.0), |_, _| true),
        DVec3::ZERO
    );
}

#[test]
fn a_boat_shoves_what_stands_beside_its_floor_but_not_a_rider_on_deck() {
    let mut entities = Entities::default();
    entities.apply(add(5, &EntityType::OAK_BOAT, [0.0, 64.0, 0.0]), 1);
    let beside = entities.push_on_player(&player_box(0.9, 64.0, 0.0), |_, _| true);
    assert!(beside.x > 0.0, "{beside:?}");
    let on_deck = entities.push_on_player(&player_box(0.5, 64.2, 0.0), |_, _| true);
    assert_eq!(on_deck, DVec3::ZERO);
}

#[test]
fn a_block_that_lands_is_held_up_by_the_block_under_it() {
    let level = level_with_floor(Some(60));
    let mut entities = Entities::default();
    entities.apply(add(8, &EntityType::FALLING_BLOCK, [8.5, 64.0, 8.5]), 1);
    for _ in 0..60 {
        entities.tick(true);
        entities.simulate(&level);
    }
    assert_eq!(entities.get(8).unwrap().support.block, Some((8, 60, 8)));
}

#[test]
fn an_armor_stand_reads_its_flags_and_limb_angles() {
    use crate::entity::appearance::{STAND_ARMS, STAND_MARKER, STAND_SMALL};
    let mut entities = Entities::default();
    entities.apply(add(8, &EntityType::ARMOR_STAND, [0.0, 64.0, 0.0]), 1);
    let stand = entities.get(8).unwrap();
    assert_eq!(stand.kind, EntityKind::ArmorStand);
    assert_eq!(stand.appearance.stand_pose[2], [-10.0, 0.0, -10.0]);
    assert!((stand.size().0 - 0.5).abs() < 1e-6 && (stand.size().1 - 1.975).abs() < 1e-6);
    entities.apply(
        EntityPacket::Data {
            id: 8,
            values: vec![
                (15, DataValue::Byte(STAND_SMALL | STAND_ARMS)),
                (16, DataValue::Rotations([30.0, 40.0, 50.0])),
            ],
        },
        1,
    );
    let stand = entities.get(8).unwrap();
    assert_eq!(stand.appearance.stand_pose[0], [30.0, 40.0, 50.0]);
    assert!(stand.appearance.stand_small());
    assert!((stand.size().1 - 0.9875).abs() < 1e-6);
    assert!(stand.pick_box().is_some());
    entities.apply(
        EntityPacket::Data {
            id: 8,
            values: vec![(15, DataValue::Byte(STAND_MARKER))],
        },
        1,
    );
    let stand = entities.get(8).unwrap();
    assert_eq!(stand.size(), (0.0, 0.0));
    assert!(stand.pick_box().is_none());
}

fn dropped(id: i32, at: [f64; 3], count: u8) -> Entities {
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;
    let mut entities = Entities::default();
    entities.apply(add(id, &EntityType::ITEM, at), 1);
    entities.apply(
        EntityPacket::Data {
            id,
            values: vec![(
                pumpkin_data::tracked_data::item::DATA_ITEM.id.0,
                DataValue::Item(Box::new(ItemStack::new(count, &Item::DIAMOND))),
            )],
        },
        1,
    );
    entities
}

#[test]
fn a_dropped_item_holds_the_stack_its_data_names() {
    let entities = dropped(5, [8.0, 70.0, 8.0], 12);
    let item = entities.get(5).unwrap();
    assert_eq!(item.kind, EntityKind::Item);
    assert_eq!(item.item.item_count, 12);
    assert!(!item.kind.is_mob());
    assert!(item.kind.simulates_movement());
}

#[test]
fn a_dropped_item_lands_on_the_floor_and_rests() {
    let level = level_with_floor(Some(60));
    let mut entities = dropped(5, [8.0, 64.0, 8.0], 1);
    for _ in 0..80 {
        entities.tick(true);
        entities.simulate(&level);
    }
    let item = entities.get(5).unwrap();
    assert!(
        (item.tick_pos().y - 61.0).abs() < 1e-6,
        "{}",
        item.tick_pos().y
    );
    assert!(item.on_ground);
    assert!(item.velocity().y.abs() < 0.1);
}

#[test]
fn an_item_the_server_holds_without_gravity_stays_in_the_air() {
    let level = level_with_floor(Some(60));
    let mut entities = dropped(5, [8.0, 64.0, 8.0], 1);
    entities.apply(
        EntityPacket::Data {
            id: 5,
            values: vec![(
                pumpkin_data::tracked_data::entity::DATA_NO_GRAVITY.id.0,
                DataValue::Bool(true),
            )],
        },
        1,
    );
    for _ in 0..40 {
        entities.tick(true);
        entities.simulate(&level);
    }
    let item = entities.get(5).unwrap();
    assert!(
        (item.tick_pos().y - 64.0).abs() < 1e-9,
        "{}",
        item.tick_pos().y
    );
    assert!(!item.on_ground);
}

#[test]
fn an_item_in_water_floats_up_slowly() {
    let mut level = level_with_floor(Some(60));
    let water = pumpkin_data::Block::WATER.default_state.id.as_u16();
    for y in 61..70 {
        level.set_block([8, y, 8], water, false);
    }
    let mut entities = dropped(5, [8.5, 62.0, 8.5], 1);
    for _ in 0..40 {
        entities.tick(true);
        entities.simulate(&level);
    }
    assert!(entities.get(5).unwrap().tick_pos().y > 61.5);
}

#[test]
fn taking_some_of_a_stack_shrinks_it_and_taking_all_removes_it() {
    let mut entities = dropped(5, [8.0, 70.0, 8.0], 10);
    entities.apply(
        EntityPacket::Take {
            item: 5,
            taker: 1,
            amount: 4,
        },
        1,
    );
    assert_eq!(entities.get(5).unwrap().item.item_count, 6);
    assert_eq!(entities.pickups().len(), 1);
    assert_eq!(entities.take_pickup_sounds().len(), 1);
    entities.apply(
        EntityPacket::Take {
            item: 5,
            taker: 1,
            amount: 6,
        },
        1,
    );
    assert!(entities.get(5).is_none());
}

#[test]
fn a_picked_up_item_flies_to_the_taker_in_three_ticks() {
    let mut entities = dropped(5, [0.0, 70.0, 0.0], 1);
    entities.apply(
        EntityPacket::Take {
            item: 5,
            taker: 1,
            amount: 1,
        },
        1,
    );
    let target = glam::DVec3::new(4.0, 70.0, 0.0);
    for _ in 0..2 {
        entities.tick_pickups(|_| Some(target));
    }
    assert_eq!(entities.pickups().len(), 1);
    let near = entities.pickups()[0].position(0.5);
    assert!(near.x > 2.0, "{near:?}");
    entities.tick_pickups(|_| Some(target));
    assert!(entities.pickups().is_empty());
}

fn launch(id: i32, kind: &EntityType, pos: [f64; 3], velocity: [f64; 3]) -> EntityPacket {
    let EntityPacket::Add(mut add) = add(id, kind, pos) else {
        unreachable!()
    };
    add.velocity = velocity;
    EntityPacket::Add(add)
}

#[test]
fn a_wither_skull_flies_on_between_the_servers_updates() {
    let level = level_with_floor(None);
    let mut entities = Entities::default();
    entities.apply(
        launch(
            9,
            &EntityType::WITHER_SKULL,
            [8.0, 100.0, 1.0],
            [0.0, 0.0, 0.1],
        ),
        1,
    );
    // `AbstractHurtingProjectile.applyInertia`: plus 0.1 along its heading,
    // then 0.95 of that.
    let mut z = 1.0;
    let mut v = 0.1;
    for _ in 0..3 {
        entities.tick(true);
        entities.simulate(&level);
        v = (v + 0.1) * f64::from(0.95_f32);
        z += v;
    }
    let at = entities.get(9).unwrap().tick_pos();
    assert!((at.z - z).abs() < 1e-9, "{} vs {z}", at.z);
    assert_eq!(at.y, 100.0, "no gravity");
}

#[test]
fn a_snowball_falls_and_stops_at_the_floor() {
    let level = level_with_floor(Some(64));
    let mut entities = Entities::default();
    entities.apply(
        launch(9, &EntityType::SNOWBALL, [8.0, 66.0, 8.0], [0.0, -1.0, 0.0]),
        1,
    );
    for _ in 0..5 {
        entities.tick(true);
        entities.simulate(&level);
    }
    let at = entities.get(9).unwrap().tick_pos();
    assert!((at.y - 65.0).abs() < 1e-6, "{at}");
}

#[test]
fn an_eye_of_ender_still_glides_to_the_servers_positions() {
    let mut entities = Entities::default();
    entities.apply(add(9, &EntityType::EYE_OF_ENDER, [8.0, 100.0, 8.0]), 1);
    assert!(!entities.get(9).unwrap().simulates());
}

#[test]
fn a_frozen_level_holds_still_what_carries_no_player() {
    let mut entities = Entities::default();
    let own = 1;
    entities.apply(add(5, &EntityType::PIG, [0.5, 64.0, 0.5]), own);
    entities.apply(add(6, &EntityType::MINECART, [4.5, 64.0, 0.5]), own);
    entities.apply(add(7, &EntityType::PLAYER, [8.5, 64.0, 0.5]), own);
    entities.apply(
        EntityPacket::Passengers {
            id: 6,
            passengers: vec![own],
        },
        own,
    );
    let step = |id| {
        EntityPacket::Move(MoveEntity {
            id,
            steps: vec![[4096, 0, 0]],
            rotation: None,
            on_ground: true,
        })
    };
    for id in [5, 7] {
        entities.apply(step(id), own);
    }
    // `TickRateManager.isEntityFrozen`: the pig stands, the other player
    // and the boat the local player sits in go on.
    entities.tick(false);
    assert!(entities.is_frozen(5));
    assert!(!entities.is_frozen(6));
    assert!(!entities.is_frozen(7));
    assert_eq!(entities.get(5).unwrap().pos(1.0).x, 0.5);
    assert!(entities.get(7).unwrap().pos(1.0).x > 8.5);
    entities.tick(true);
    assert!(!entities.is_frozen(5));
    assert!(entities.get(5).unwrap().pos(1.0).x > 0.5);
}
