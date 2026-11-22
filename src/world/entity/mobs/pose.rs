//! What a mob's pose reads each frame ([`PoseInput`]) and the transform

use super::*;

/// Everything the pose of `entity` reads this frame.
pub fn pose_input(entity: &ClientEntity, partial: f32) -> PoseInput {
    let mut living = entity.living_state(partial);
    if entity.kind == EntityKind::Enderman && entity.appearance.creepy {
        let (dx, dz) = enderman::creepy_shake(living.age_in_ticks, 1.0);
        living.position.x += dx;
        living.position.z += dz;
    }
    let mut humanoid = HumanoidRenderState::from_living(&living);
    let a = &entity.appearance;
    let main_arm = a.main_arm(entity.kind);
    humanoid.crouching = a.crouching();
    humanoid.swim_amount = living.swim.amount;
    humanoid.using_item = a.using_hand().is_some();
    humanoid.drowned_swim = entity.kind == EntityKind::Drowned;
    humanoid.swing = entity.swing.current().map(|(hand, swing)| SwingPose {
        arm: hand.arm(main_arm),
        animation: entity.swing.animation(partial),
        whack: swing.whack,
    });
    let main_hand = entity.equipment.get(EquipmentSlot::MainHand);
    let holding_bow = main_hand.item.id == Item::BOW.id;
    let mut skeleton_melee = false;
    let mut piglin = piglin::PiglinPose::default();
    let mut villager = villager::VillagerPose::default();
    let mut illager = illager::IllagerPose::default();
    let mut wolf = wolf::WolfPose::default();
    let mut feline = feline::FelinePose::default();
    let mut equine = horse::EquinePose::default();
    let mut golem = golem::GolemPose::default();
    let bat_resting = a.resting;
    let mut enderman = enderman::EndermanPose::default();
    let mut fox_pose = fox::FoxPose::default();
    match entity.kind {
        EntityKind::Zombie
        | EntityKind::Husk
        | EntityKind::Drowned
        | EntityKind::ZombieVillager
        | EntityKind::ZombifiedPiglin
        | EntityKind::Giant => {
            humanoid.attack = AttackStyle::Zombie {
                aggressive: a.aggressive(),
                baby_holding: a.baby && !main_hand.is_empty(),
            };
            if entity.kind == EntityKind::Drowned
                && a.aggressive()
                && main_hand.item.id == Item::TRIDENT.id
            {
                match main_arm {
                    Arm::Right => humanoid.right_arm_pose = ArmPose::ThrowTrident,
                    Arm::Left => humanoid.left_arm_pose = ArmPose::ThrowTrident,
                }
            }
        }
        EntityKind::Villager | EntityKind::WanderingTrader => {
            villager.unhappy = a.unhappy;
        }
        EntityKind::Witch => {
            villager.witch = Some(villager::WitchPose {
                entity_id: entity.id,
                holding_item: !main_hand.is_empty(),
            });
        }
        EntityKind::Piglin | EntityKind::PiglinBrute => {
            let off_hand = entity.equipment.get(EquipmentSlot::OffHand);
            let melee =
                a.aggressive() && !main_hand.is_empty() && main_hand.item.id != Item::CROSSBOW.id;
            piglin.arms = if a.dancing {
                piglin::ArmsPose::Dancing
            } else if entity.kind == EntityKind::Piglin
                && off_hand
                    .item
                    .has_tag(&pumpkin_data::tag::Item::MINECRAFT_PIGLIN_LOVED)
            {
                piglin::ArmsPose::Admiring
            } else if melee {
                piglin::ArmsPose::Melee {
                    swing: humanoid.swing.map_or(0.0, |s| s.animation),
                }
            } else if a.charging_crossbow {
                piglin::ArmsPose::CrossbowCharge {
                    ticks: entity.ticks_using_item(partial),
                    max: CROSSBOW_CHARGE_TICKS,
                }
            } else if main_hand.item.id == Item::CROSSBOW.id
                && main_hand
                    .get_data_component::<ChargedProjectilesImpl>()
                    .is_some_and(|c| !c.projectiles.is_empty())
            {
                piglin::ArmsPose::CrossbowHold
            } else {
                piglin::ArmsPose::Default
            };
            piglin.main_arm = main_arm;
            if matches!(piglin.arms, piglin::ArmsPose::Melee { swing } if swing > 0.0) {
                humanoid.swing = None;
            }
        }
        k if k.is_equine() => {
            let amounts = entity.equine_amounts(partial);
            equine = horse::EquinePose {
                eating: amounts.eat,
                standing: amounts.stand,
                feeding: amounts.mouth,
                animate_tail: false,
                has_chest: a.has_chest,
                in_water: false,
            };
        }
        EntityKind::IronGolem => golem = entity.golem_pose(partial),
        EntityKind::Enderman => {
            enderman = enderman::EndermanPose {
                carrying: a.carrying(),
                creepy: a.creepy,
            };
        }
        EntityKind::Wolf => {
            wolf = wolf::WolfPose {
                angry: a.angry,
                sitting: a.sitting(),
                tail_angle: wolf::tail_angle(a.angry, a.tame(), a.health),
                head_roll: entity.head_roll(partial),
            };
        }
        EntityKind::Cat | EntityKind::Ocelot => {
            let amounts = entity.feline_amounts(partial);
            feline = feline::FelinePose {
                crouching: a.crouching(),
                sprinting: a.sprinting(),
                sitting: entity.kind == EntityKind::Cat && a.sitting(),
                lie_down: amounts.lie_down,
                lie_down_tail: amounts.lie_down_tail,
                relax_one: amounts.relax_one,
            };
        }
        EntityKind::Pillager
        | EntityKind::Vindicator
        | EntityKind::Evoker
        | EntityKind::Illusioner => {
            let swing = humanoid.swing.map_or(0.0, |s| s.animation);
            let weapon = !main_hand.is_empty();
            illager.hat = entity.kind == EntityKind::Illusioner;
            illager.arms = match entity.kind {
                EntityKind::Evoker | EntityKind::Illusioner if a.casting_spell => {
                    illager::ArmPose::Spellcasting
                }
                EntityKind::Pillager if a.charging_crossbow => illager::ArmPose::CrossbowCharge {
                    ticks: entity.ticks_using_item(partial),
                    max: CROSSBOW_CHARGE_TICKS,
                },
                EntityKind::Pillager if main_hand.item.id == Item::CROSSBOW.id => {
                    illager::ArmPose::CrossbowHold
                }
                EntityKind::Evoker | EntityKind::Illusioner => {
                    if a.celebrating {
                        illager::ArmPose::Celebrating
                    } else {
                        illager::ArmPose::Crossed
                    }
                }
                EntityKind::Pillager if !a.aggressive() => illager::ArmPose::Neutral,
                _ if a.aggressive() => illager::ArmPose::Attacking { weapon, swing },
                _ if a.celebrating => illager::ArmPose::Celebrating,
                _ => illager::ArmPose::Crossed,
            };
            illager.main_arm = main_arm;
        }
        EntityKind::Stray
        | EntityKind::Skeleton
        | EntityKind::WitherSkeleton
        | EntityKind::Bogged
        | EntityKind::Parched => {
            if a.aggressive() && holding_bow {
                match main_arm {
                    Arm::Right => humanoid.right_arm_pose = ArmPose::BowAndArrow,
                    Arm::Left => humanoid.left_arm_pose = ArmPose::BowAndArrow,
                }
            }
            skeleton_melee = a.aggressive() && !holding_bow;
        }
        EntityKind::Fox => {
            fox_pose = fox::FoxPose {
                sitting: a.fox_flags & fox::FLAG_SITTING != 0,
                sleeping: a.fox_flags & fox::FLAG_SLEEPING != 0,
                crouching: a.fox_flags & fox::FLAG_CROUCHING != 0,
                pouncing: a.fox_flags & fox::FLAG_POUNCING != 0,
                faceplanted: a.fox_flags & fox::FLAG_FACEPLANTED != 0,
                crouch_amount: entity.fox_crouch(partial),
                head_roll: entity.head_roll(partial),
            };
        }
        _ => {}
    }
    PoseInput {
        thing: entity.thing_pose(partial),
        living,
        humanoid,
        skeleton_melee,
        flap: entity.flap(partial),
        squish: entity.squish(partial),
        piglin,
        villager,
        illager,
        wolf,
        feline,
        equine,
        golem,
        in_water: entity.in_water,
        moving: entity.moving,
        turtle: turtle::TurtlePose {
            on_land: !entity.in_water && entity.on_ground,
            laying_egg: a.laying_egg,
            has_egg: !a.baby && a.has_egg,
        },
        guardian: if matches!(
            entity.kind,
            EntityKind::Guardian | EntityKind::ElderGuardian
        ) {
            entity.guardian_pose(partial)
        } else {
            Default::default()
        },
        axolotl: if entity.kind == EntityKind::Axolotl {
            entity.axolotl_factors(partial)
        } else {
            Default::default()
        },
        squid_angle: if entity.kind.is_squid() {
            entity.squid_pose(partial).0
        } else {
            0.0
        },
        happy_ghast: ghast::HappyGhastPose {
            harnessed: !entity.equipment.get(EquipmentSlot::Body).is_empty(),
            ridden: false,
        },
        anim: entity.anim,
        bat_resting,
        enderman,
        rowing: if entity.kind == EntityKind::Boat {
            entity.rowing(partial)
        } else {
            Default::default()
        },
        fox: fox_pose,
        polar_stand: if entity.kind == EntityKind::PolarBear {
            entity.polar_stand(partial)
        } else {
            0.0
        },
        armadillo_hiding: entity.kind == EntityKind::Armadillo
            && armadillo::hide_in_shell(a.armadillo_state, entity.armadillo_ticks()),
        camel_jump: 0.0,
        frog_croak: entity.kind == EntityKind::Frog && a.pose == frog::POSE_CROAKING,
        sheared: a.sheared(),
        goat: goat::GoatPose {
            has_left_horn: a.goat_left_horn,
            has_right_horn: a.goat_right_horn,
            ramming_x_head_rot: 0.0,
        },
        llama_chest: matches!(entity.kind, EntityKind::Llama | EntityKind::TraderLlama)
            && a.has_chest,
        panda: panda::pose_from_flags(a.panda_flags),
        parrot: parrot::pose(
            false,
            entity.kind == EntityKind::Parrot && a.sitting(),
            entity.kind == EntityKind::Parrot && !entity.on_ground,
        ),
        parrot_flap: entity.flap(partial).0,
        hoglin: hoglin::HoglinPose::default(),
        bee: bee::BeePose {
            roll_amount: if a.bee_flags & bee::FLAG_ROLL != 0 {
                1.0
            } else {
                0.0
            },
            has_stinger: a.bee_flags & bee::FLAG_HAS_STUNG == 0,
            on_ground: entity.on_ground,
            angry: a.angry,
            nectar: a.bee_flags & bee::FLAG_HAS_NECTAR != 0,
        },
        strider: strider::StriderPose {
            suffocating: a.strider_cold,
            ridden: false,
        },
        ravager: ravager::RavagerPose::default(),
        shulker: shulker::ShulkerPose {
            peek: shulker::peek_from_data(a.shulker_peek),
            attach: a.attach_face,
        },
        wither: wither::WitherPose {
            invul: a.wither_invul > 0,
        },
        vex: vex::VexPose {
            charging: a.charging,
            right_item: !entity.equipment.get(EquipmentSlot::MainHand).is_empty(),
            left_item: !entity.equipment.get(EquipmentSlot::OffHand).is_empty(),
        },
        allay: allay::AllayPose {
            dancing: a.dancing,
            spinning: false,
            spinning_progress: 0.0,
            holding: if entity.kind == EntityKind::Allay && !main_hand.is_empty() {
                1.0
            } else {
                0.0
            },
        },
        dragon: dragon::DragonPose {
            flap_time: (living.age_in_ticks * 0.1).fract(),
        },
        phantom_flap: phantom::flap_time(entity.id, living.age_in_ticks),
        sniffer_searching: a.sniffer_state == sniffer::STATE_SEARCHING,
        warden_tendril: entity.warden.tendril(partial),
        creaking_move: a.creaking_active,
        copper_holding: entity.kind == EntityKind::CopperGolem && !main_hand.is_empty(),
        armor_stand: armor_stand::StandPoseInput::of(a, entity.yaw_degrees(partial)),
    }
}

/// From model space to the world: a vehicle sits on its own turn, a living
pub fn root(entity: &ClientEntity, input: &PoseInput, partial: f32) -> glam::Mat4 {
    use crate::world::entity::living;
    let position = input.living.position;
    match entity.kind {
        EntityKind::Boat => vehicle::boat_root(
            position,
            entity.yaw_degrees(partial),
            entity.vehicle_shake(partial),
        ),
        EntityKind::Minecart => vehicle::minecart_root(
            position,
            entity.yaw_degrees(partial),
            entity.pitch_degrees(partial),
            entity.vehicle_shake(partial),
        ),
        kind if kind.is_projectile()
            || matches!(kind, EntityKind::EndCrystal | EntityKind::Cushion) =>
        {
            projectile::root(
                kind,
                position,
                entity.yaw_degrees(partial),
                entity.pitch_degrees(partial),
                entity.age_in_ticks(partial),
            )
        }
        _ => living::root_transform(&input.living, placement(entity, partial)),
    }
}

/// `CrossbowItem.getChargeDuration` without Quick Charge (1.25 seconds).
pub(super) const CROSSBOW_CHARGE_TICKS: f32 = 25.0;
