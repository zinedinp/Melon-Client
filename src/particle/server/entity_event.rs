//! The particles of `ClientboundEntityEventPacket` (each entity class's `handleEntityEvent`).

use glam::DVec3;

use super::super::base::Particle;
use super::super::motion::emitter::Emitter;
use super::super::{EntityBox, Kind, ParticleEngine, ParticleSpec, Spawn, World};
use crate::particle::client::entity as own;
use crate::world::java_random::JavaRandom;

/// `EntityEvent` ids that spawn particles.
pub mod id {
    pub const PROJECTILE_HIT: u8 = 3;
    pub const TAMING_FAILED: u8 = 6;
    pub const TAMING_SUCCEEDED: u8 = 7;
    pub const LOVE_HEARTS: u8 = 12;
    pub const VILLAGER_ANGRY: u8 = 13;
    pub const VILLAGER_HAPPY: u8 = 14;
    pub const WITCH_HAT_MAGIC: u8 = 15;
    pub const IN_LOVE_HEARTS: u8 = 18;
    pub const SILVERFISH_MERGE_ANIM: u8 = 20;
    pub const TALISMAN_ACTIVATE: u8 = 35;
    pub const DOLPHIN_LOOKING_FOR_TREASURE: u8 = 38;
    pub const TRUSTING_FAILED: u8 = 40;
    pub const TRUSTING_SUCCEEDED: u8 = 41;
    pub const VILLAGER_SWEAT: u8 = 42;
    pub const TELEPORT: u8 = 46;
    pub const JUMP: u8 = 1;
    pub const FOX_EAT: u8 = 45;
    pub const MAINHAND_BREAK: u8 = 47;
    pub const OFFHAND_BREAK: u8 = 48;
    pub const HEAD_BREAK: u8 = 49;
    pub const CHEST_BREAK: u8 = 50;
    pub const LEGS_BREAK: u8 = 51;
    pub const FEET_BREAK: u8 = 52;
    pub const HONEY_SLIDE: u8 = 53;
    pub const HONEY_JUMP: u8 = 54;
    pub const POOF: u8 = 60;
    pub const BODY_BREAK: u8 = 65;
    pub const DROWN_PARTICLES: u8 = 67;
    pub const SADDLE_BREAK: u8 = 68;
}

/// `ClientboundAnimatePacket` actions with particles.
pub mod animate {
    pub const CRITICAL_HIT: u8 = 1;
    pub const MAGIC_CRITICAL_HIT: u8 = 2;
}

/// The particles of entity event `event` on the entity `body` is of.
pub fn spawn(engine: &mut ParticleEngine, world: &World, entity: i32, event: u8) {
    let Some(body) = (world.entity)(entity) else {
        return;
    };
    let mut out = Vec::new();
    let rng = engine.rng();
    match event {
        id::PROJECTILE_HIT => {
            let spec = match body.name {
                "snowball" => ParticleSpec::simple(Kind::ItemSnowball),
                "egg" | "blue_egg" | "brown_egg" => {
                    ParticleSpec::item(pumpkin_data::item::Item::EGG.id)
                }
                _ => return,
            };
            for _ in 0..8 {
                out.push(Spawn::new(spec, body.pos, DVec3::ZERO));
            }
        }
        id::TAMING_FAILED | id::TRUSTING_FAILED => {
            around(&body, Kind::Smoke, 7, 0.5, rng, &mut out)
        }
        id::TAMING_SUCCEEDED | id::TRUSTING_SUCCEEDED | id::IN_LOVE_HEARTS => {
            around(&body, Kind::Heart, 7, 0.5, rng, &mut out);
        }
        id::LOVE_HEARTS => around(&body, Kind::Heart, 5, 1.0, rng, &mut out),
        id::VILLAGER_ANGRY => around(&body, Kind::AngryVillager, 5, 1.0, rng, &mut out),
        id::VILLAGER_HAPPY | id::DOLPHIN_LOOKING_FOR_TREASURE => {
            around(&body, Kind::HappyVillager, 5, 1.0, rng, &mut out);
        }
        id::VILLAGER_SWEAT => around(&body, Kind::Splash, 5, 1.0, rng, &mut out),
        id::WITCH_HAT_MAGIC => {
            let mut i = 0;
            while i < rng.next_int(35) + 10 {
                let spread = f64::from(0.13f32);
                let at = DVec3::new(
                    body.pos.x + rng.next_gaussian() * spread,
                    body.pos.y + f64::from(body.height) + 0.5 + rng.next_gaussian() * spread,
                    body.pos.z + rng.next_gaussian() * spread,
                );
                out.push(Spawn::simple(Kind::Witch, at, DVec3::ZERO));
                i += 1;
            }
        }
        id::SILVERFISH_MERGE_ANIM | id::POOF => {
            for _ in 0..20 {
                let aux = DVec3::new(
                    rng.next_gaussian() * 0.02,
                    rng.next_gaussian() * 0.02,
                    rng.next_gaussian() * 0.02,
                );
                let at = body.random_point(rng) - aux * 10.0;
                out.push(Spawn::simple(Kind::Poof, at, aux));
            }
        }
        id::TELEPORT => {
            for i in 0..128 {
                let d = f64::from(i) / 127.0;
                let mut v = || f64::from((rng.next_float() - 0.5) * 0.2);
                let aux = DVec3::new(v(), v(), v());
                let along = body.prev.lerp(body.pos, d);
                let w = f64::from(body.width);
                let at = along
                    + DVec3::new(
                        (rng.next_double() - 0.5) * w * 2.0,
                        rng.next_double() * f64::from(body.height),
                        (rng.next_double() - 0.5) * w * 2.0,
                    );
                out.push(Spawn::simple(Kind::Portal, at, aux));
            }
        }
        id::TALISMAN_ACTIVATE => {
            tracking(
                engine,
                entity,
                ParticleSpec::simple(Kind::TotemOfUndying),
                30,
            );
            return;
        }
        id::MAINHAND_BREAK
        | id::OFFHAND_BREAK
        | id::HEAD_BREAK
        | id::CHEST_BREAK
        | id::LEGS_BREAK
        | id::FEET_BREAK
        | id::BODY_BREAK
        | id::SADDLE_BREAK => {
            use crate::network::play::game::EquipmentSlot as Slot;
            let slot = match event {
                id::MAINHAND_BREAK => Slot::MainHand,
                id::OFFHAND_BREAK => Slot::OffHand,
                id::HEAD_BREAK => Slot::Head,
                id::CHEST_BREAK => Slot::Chest,
                id::LEGS_BREAK => Slot::Legs,
                id::FEET_BREAK => Slot::Feet,
                id::BODY_BREAK => Slot::Body,
                _ => Slot::Saddle,
            };
            let Some(item) = (world.equipment)(entity, slot) else {
                return;
            };
            rng.next_float();
            let eye = body.eye();
            own::item_crumbs(item, eye, body.yaw, body.pitch, 5, rng, &mut out);
        }
        id::FOX_EAT => {
            let Some(item) =
                (world.equipment)(entity, crate::network::play::game::EquipmentSlot::MainHand)
            else {
                return;
            };
            let spec = ParticleSpec::item(item);
            let (yaw, pitch) = (body.yaw.to_radians(), body.pitch.to_radians());
            let look_x = f64::from(-yaw.sin() * pitch.cos());
            let look_z = f64::from(yaw.cos() * pitch.cos());
            let at = body.pos + DVec3::new(look_x / 2.0, 0.0, look_z / 2.0);
            for _ in 0..8 {
                let d = DVec3::new(
                    (f64::from(rng.next_float()) - 0.5) * 0.1,
                    f64::from(rng.next_float()) * 0.1 + 0.1,
                    0.0,
                );
                let d = own::y_rot(
                    own::x_rot(d, -body.pitch.to_radians()),
                    -body.yaw.to_radians(),
                );
                out.push(Spawn::new(spec, at, DVec3::new(d.x, d.y + 0.05, d.z)));
            }
        }
        id::HONEY_SLIDE | id::HONEY_JUMP => {
            let honey = pumpkin_data::Block::HONEY_BLOCK.default_state.id.as_u16();
            let count = if event == id::HONEY_SLIDE { 5 } else { 10 };
            for _ in 0..count {
                out.push(Spawn::new(
                    ParticleSpec::block(Kind::Block, honey),
                    body.pos,
                    DVec3::ZERO,
                ));
            }
        }
        id::DROWN_PARTICLES => {
            for _ in 0..8 {
                let mut triangle = || rng.next_double() - rng.next_double();
                let at = body.pos + DVec3::new(triangle(), triangle(), triangle());
                out.push(Spawn::simple(Kind::Bubble, at, body.vel));
            }
        }
        id::JUMP if body.name == "rabbit" => {
            let on = [
                body.pos.x.floor() as i32,
                (body.pos.y - 0.2).floor() as i32,
                body.pos.z.floor() as i32,
            ];
            let state = world.level.block_state(on[0], on[1], on[2]).unwrap_or(0);
            own::sprint_dust(body.pos, body.width, body.vel, (on, state), rng, &mut out);
        }
        _ => return,
    }
    engine.request_all(out);
}

/// `ClientboundAnimatePacket`: a critical hit's sparks around `entity`.
pub fn animate(engine: &mut ParticleEngine, entity: i32, action: u8) {
    let kind = match action {
        animate::CRITICAL_HIT => Kind::Crit,
        animate::MAGIC_CRITICAL_HIT => Kind::EnchantedHit,
        _ => return,
    };
    tracking(engine, entity, ParticleSpec::simple(kind), 3);
}

/// `ParticleEngine.createTrackingEmitter`.
pub fn tracking(engine: &mut ParticleEngine, entity: i32, spec: ParticleSpec, lifetime: i32) {
    let rng = engine.rng();
    let mut p = Particle::at(Kind::Poof, DVec3::ZERO, rng);
    p.lifetime = lifetime;
    p.behavior = super::super::behavior::Behavior::Emitter(Emitter::Tracking { entity, spec });
    engine.add(p);
}

/// `count` of `kind` in the box, raised by `lift` (`spawnTamingParticles` and its kin).
fn around(
    body: &EntityBox,
    kind: Kind,
    count: u32,
    lift: f64,
    rng: &mut JavaRandom,
    out: &mut Vec<Spawn>,
) {
    for _ in 0..count {
        let aux = DVec3::new(
            rng.next_gaussian() * 0.02,
            rng.next_gaussian() * 0.02,
            rng.next_gaussian() * 0.02,
        );
        let at = body.random_point(rng) + DVec3::Y * lift;
        out.push(Spawn::simple(kind, at, aux));
    }
}
