//! How the renderer places a mob.

use super::*;

/// How the renderer places the model.
pub fn placement(entity: &ClientEntity, partial: f32) -> Placement {
    let look = entity.kind.look();
    let adult = !entity.appearance.baby;
    let mut placement = Placement {
        scale: Vec3::splat(if adult || !look.adult_scale {
            look.scale
        } else {
            1.0
        }),
        flip_degrees: look.flip_degrees,
        ..Default::default()
    };
    match entity.kind {
        EntityKind::Creeper => placement.scale = creeper::swell_scale(entity.swelling(partial)),
        EntityKind::Cat => placement.lie_down = entity.feline_amounts(partial).lie_down,
        EntityKind::SulfurCube => {
            let a = &entity.appearance;
            let (scale, offset) = sulfur_cube::placement(
                a.size as f32,
                entity.squish(partial),
                sulfur_cube::swallowed_block(entity).is_some(),
                sulfur_cube::fuse_remaining(entity.fuse_ticks, partial),
                a.baby,
                a.invisible(),
            );
            placement.scale = scale;
            placement.offset = offset;
        }
        EntityKind::Slime | EntityKind::MagmaCube => {
            let size = entity.appearance.size as f32;
            let squish = entity.squish(partial);
            let w = 1.0 / (squish / (size * 0.5 + 1.0) + 1.0);
            let hair = if entity.kind == EntityKind::Slime {
                0.999
            } else {
                1.0
            };
            placement.scale = Vec3::new(w * size, size / w, w * size) * hair;
        }
        EntityKind::Cod => {
            placement.fish = Some(fish::Wobble::cod(
                entity.age_in_ticks(partial),
                entity.in_water,
            ));
        }
        EntityKind::Salmon => {
            placement.fish = Some(fish::Wobble::salmon(
                entity.age_in_ticks(partial),
                entity.in_water,
            ));
        }
        EntityKind::TropicalFish => {
            placement.fish = Some(fish::Wobble::tropical(
                entity.age_in_ticks(partial),
                entity.in_water,
            ));
        }
        EntityKind::Pufferfish => {
            placement.fish = Some(fish::Wobble::pufferfish(entity.age_in_ticks(partial)));
        }
        EntityKind::Squid | EntityKind::GlowSquid => {
            placement.squid = Some(entity.squid_pose(partial).1);
        }
        EntityKind::IronGolem => {
            placement.roll_degrees =
                golem::roll(entity.walk.position(partial), entity.walk.speed(partial));
        }
        EntityKind::Fox
            if entity.appearance.fox_flags & (fox::FLAG_POUNCING | fox::FLAG_FACEPLANTED) != 0 =>
        {
            placement.extra_pitch = -entity.pitch_degrees(partial);
        }
        EntityKind::Phantom => {
            placement.scale *= phantom::size_scale(entity.appearance.size);
        }
        _ => {}
    }
    placement
}

/// How far a hurt body is mixed toward red.
pub const HURT_OVERLAY: f32 = 1.0 - 178.0 / 255.0;

/// `hasRedOverlay`: the overlay of a body that is hurt or dying.
pub const fn hurt_overlay(hurt: bool) -> f32 {
    if hurt { HURT_OVERLAY } else { 0.0 }
}

/// The overlay this frame: hurt red wins over a creeper's white flash.
pub fn overlay(entity: &ClientEntity, state: &LivingRenderState, partial: f32) -> f32 {
    if state.hurt {
        return hurt_overlay(true);
    }
    if entity.kind == EntityKind::Creeper {
        return -creeper::white_flash(entity.swelling(partial));
    }
    0.0
}

/// `GlowSquidRenderer.getBlockLightLevel`.
pub fn block_glow(entity: &ClientEntity) -> u8 {
    match entity.kind {
        EntityKind::MagmaCube
        | EntityKind::Blaze
        | EntityKind::WitherSkull
        | EntityKind::ShulkerBullet => 15,
        EntityKind::DragonFireball => 15,
        EntityKind::ThrownItem if projectile::thrown_look(entity.ty).1 => 15,
        EntityKind::GlowSquid => {
            let lit = 1.0 - entity.appearance.dark_ticks as f32 / 10.0;
            (lit.clamp(0.0, 1.0) * 15.0) as u8
        }
        _ => 0,
    }
}

/// `LivingEntity.getAgeScale`: how much smaller a baby is, by kind.
pub fn age_scale(entity: &ClientEntity) -> f32 {
    if !entity.appearance.baby {
        return 1.0;
    }
    match entity.kind {
        EntityKind::Armadillo | EntityKind::Camel | EntityKind::CamelHusk => 0.6,
        EntityKind::Turtle => 0.3,
        EntityKind::Dolphin => 0.65,
        EntityKind::HappyGhast => 0.2375,
        EntityKind::Goat => 0.55,
        _ => 0.5,
    }
}

/// `EntityRenderer.getShadowRadius` as each renderer has it: the
/// renderer's `shadowRadius`, times the age scale for a `MobRenderer`
/// (and the extra halving some give their young), or worked out from
/// the size for the cube mobs and pufferfish. `LivingEntityRenderer`
/// would multiply by the `scale` attribute too, which is not tracked
/// for other entities (1).
pub fn shadow_radius(entity: &ClientEntity) -> f32 {
    let a = &entity.appearance;
    match entity.kind {
        // `AbstractCubeMobRenderer` / `PufferfishRenderer` replace the
        // radius outright: no age scale.
        k if k.is_cube() => 0.25 * a.size as f32,
        EntityKind::Pufferfish => 0.1 + 0.1 * a.puff_state as f32,
        kind => {
            // `LivingEntityRenderer.getShadowRadius`: times the scale.
            let radius = kind.look().shadow * entity.entity_scale();
            if !kind.is_mob() {
                return radius;
            }
            let radius = radius * age_scale(entity);
            match kind {
                EntityKind::Villager | EntityKind::Strider if a.baby => radius * 0.5,
                EntityKind::Turtle if a.baby => radius * 0.83,
                _ => radius,
            }
        }
    }
}

/// `EntityRenderer.getShadowStrength`: items and orbs cast a fainter one.
pub fn shadow_strength(entity: &ClientEntity) -> f32 {
    match entity.kind {
        EntityKind::Item | EntityKind::ExperienceOrb => 0.75,
        _ => 1.0,
    }
}
