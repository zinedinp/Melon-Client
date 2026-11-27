//! Heads: player heads that name a profile, drawn with that player's skin.

use glam::{DVec3, IVec3, Mat4, Vec3};
use pumpkin_data::item_stack::ItemStack;

use super::InWorld;
use super::entities::Placed;
use super::entity_models::DrawStyle;
use super::world_resources::WorldResources;
use melon_core::math::angle::Rotation;
use melon_render::{EntityDraw, Glint, RenderLayer, Renderer};
use melon_world::block_entity::models::SkullType;
use melon_world::block_entity::{self, Kind, VIEW_DISTANCE, kind_of};
use melon_world::culling::Culler;
use melon_world::entity::humanoid::ArmorSlot;
use melon_world::entity::mobs;
use melon_world::entity::mobs::custom_head::HeadTransforms;
use melon_world::entity::model::{Model, ModelPose, PartId};
use melon_world::head_profile::HeadProfile;
use melon_world::item_model::HandPose;
use melon_world::level::Level;
use melon_world::lightmap::LightmapParams;

/// `CustomHeadLayer`'s skull scale over the wearer's head.
const WORN_SCALE: f32 = 1.1875;

/// The placed heads with a profile `culler` lets through, near `eye`.
pub(super) fn visible_heads(level: &Level, culler: &Culler) -> Vec<[i32; 3]> {
    let eye = culler.eye();
    level
        .heads()
        .map(|(pos, _)| pos)
        .filter(|&pos| {
            let min = IVec3::from(pos).as_dvec3();
            let center = min + DVec3::splat(0.5);
            center.distance_squared(eye) as f32 <= VIEW_DISTANCE * VIEW_DISTANCE
                && culler.sees_box(min, min + DVec3::ONE)
                && culler.reaches(culler.section_of(center))
        })
        .collect()
}

impl InWorld {
    /// Asks for the skins of the heads near the player, and uploads those that arrived.
    pub(super) fn prepare_heads(&self, renderer: &mut Renderer, assets: &mut WorldResources) {
        let eye = self.player.tick_eye().as_vec3();
        for (pos, profile) in self.level.heads() {
            let center = IVec3::from(pos).as_vec3() + Vec3::splat(0.5);
            if center.distance_squared(eye) <= VIEW_DISTANCE * VIEW_DISTANCE {
                assets.players.request_head(profile);
            }
        }
        let worn = self
            .entities
            .iter()
            .map(|entity| entity.equipment.armor()[0])
            .chain([self.own_armor()[0]]);
        for stack in worn {
            prepare_worn_head(renderer, assets, stack);
        }
    }

    /// The draws of the placed heads in `visible` (from [`visible_heads`]).
    pub(super) fn head_draws(
        &self,
        assets: &WorldResources,
        visible: &[[i32; 3]],
        params: &LightmapParams,
        out: &mut Vec<EntityDraw>,
    ) {
        for &pos in visible {
            let Some(profile) = self.level.head_profile(pos) else {
                continue;
            };
            let Some(Kind::Skull { placement, .. }) = self
                .level
                .block_state(pos[0], pos[1], pos[2])
                .and_then(kind_of)
            else {
                continue;
            };
            let origin = IVec3::from(pos);
            let center = origin.as_vec3() + Vec3::splat(0.5);
            let light = self.light_at_eye(center, 0, params);
            let look = assets.players.for_head(Some(profile));
            assets.players.head.draws(
                SkullType::Player,
                0.0,
                block_entity::skull_transform(*placement),
                origin,
                DrawStyle {
                    texture: look.texture,
                    layer: RenderLayer::Entities,
                    light: Some(light),
                    overlay: 0.0,
                    glint: Glint::None,
                },
                out,
            );
        }
    }
}

/// Who wears a head slot and where.
pub(super) struct Wearer<'a> {
    pub model: &'a Model,
    pub pose: &'a ModelPose,
    pub head: PartId,
    pub transforms: HeadTransforms,
    pub walk_position: f32,
}

impl Wearer<'_> {
    /// The head part's matrix, through the layer's scale.
    fn head(&self, root: Mat4) -> Mat4 {
        let t = self.transforms;
        let scale = Vec3::new(t.horizontal_scale, t.vertical_scale, t.horizontal_scale);
        self.model
            .part_transform(self.pose, self.head, root * Mat4::from_scale(scale))
    }
}

/// What [`submit_worn_head`] places a worn head with.
pub(super) fn worn_skull_transform(head: Mat4, transforms: HeadTransforms) -> Mat4 {
    head * Mat4::from_translation(Vec3::new(0.0, transforms.skull_y_offset, 0.0))
        * Mat4::from_scale(Vec3::splat(WORN_SCALE))
}

/// `CustomHeadLayer`: what is worn on the head and isn't armor.
pub(super) fn submit_worn_head(
    assets: &WorldResources,
    stack: &ItemStack,
    wearer: &Wearer,
    root: Mat4,
    placed: Placed,
    out: &mut Vec<EntityDraw>,
) {
    if stack.is_empty() {
        return;
    }
    let Placed {
        origin,
        layer,
        light,
    } = placed;
    let head = wearer.head(root);
    let style = |texture| DrawStyle {
        texture,
        layer,
        light: Some(light),
        overlay: 0.0,
        glint: Glint::None,
    };
    if let Some(skull) = SkullType::of_item(stack.item.registry_key) {
        let texture = match skull {
            SkullType::Player => {
                assets
                    .players
                    .for_head(HeadProfile::of_stack(stack).as_ref())
                    .texture
            }
            _ => match assets.mobs.texture(skull.texture()) {
                Some(texture) => texture,
                None => return,
            },
        };
        assets.players.head.draws(
            skull,
            wearer.walk_position,
            worn_skull_transform(head, wearer.transforms),
            origin,
            style(texture),
            out,
        );
        return;
    }
    if worn_as_armor(stack) {
        return;
    }
    let parts: Vec<_> = assets.held.get(stack, HandPose::Head, false).collect();
    let lift = -0.25 + wearer.transforms.y_offset;
    for &(mesh, atlas, translucent) in &parts {
        out.push(EntityDraw {
            scroll: None,
            alpha: 1.0,
            mesh,
            atlas,
            model: head
                * Mat4::from_translation(Vec3::new(0.0, lift, 0.0))
                * Mat4::rot_y_deg(180.0)
                * Mat4::from_scale(Vec3::new(0.625, -0.625, -0.625)),
            origin,
            layer,
            blend: melon_render::Blending::of(translucent),
            light: Some(light),
            overlay: 0.0,
            glint: Glint::entity(
                melon_world::glint::has_foil(stack),
                assets.held.glint_texels(atlas),
            ),
        });
    }
}

/// `HumanoidArmorLayer.shouldRender` for the head.
fn worn_as_armor(stack: &ItemStack) -> bool {
    mobs::wears_as_armor(stack, ArmorSlot::Head)
}

/// Gets ready what [`submit_worn_head`] draws for `stack`.
pub(super) fn prepare_worn_head(
    renderer: &mut Renderer,
    assets: &mut WorldResources,
    stack: &ItemStack,
) {
    if stack.is_empty() {
        return;
    }
    match SkullType::of_item(stack.item.registry_key) {
        Some(SkullType::Player) => {
            if let Some(profile) = HeadProfile::of_stack(stack) {
                assets.players.request_head(&profile);
            }
        }
        Some(skull) => assets.mobs.prepare_texture(renderer, skull.texture()),
        None if !worn_as_armor(stack) => assets.held.prepare(renderer, stack, false),
        None => {}
    }
}

/// The faces `object` components in text show.
pub fn sync_head_objects(
    font: &melon_gui::text::font::Font,
    players: &mut super::entity_models::PlayerModels,
) {
    use melon_protocol::component::ObjectInfo;
    use melon_gui::text::objects::ObjectQuads;
    for player in font.player_objects() {
        let profile = &player.profile;
        let head = (!profile.is_empty()).then(|| HeadProfile {
            id: profile.id,
            name: profile.name.as_deref().map(Into::into),
            textures: profile.textures.as_deref().map(Into::into),
        });
        if let Some(head) = &head {
            players.request_head(head);
        }
        let texture = players.for_head(head.as_ref()).texture;
        font.set_object(
            ObjectInfo::Player(player.clone()),
            Some(ObjectQuads::head(texture, player.hat)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::item::Item;

    #[test]
    fn a_helmet_is_armor_and_a_pumpkin_or_a_head_is_worn_on_the_head() {
        assert!(worn_as_armor(&ItemStack::new(1, &Item::IRON_HELMET)));
        assert!(!worn_as_armor(&ItemStack::new(1, &Item::CARVED_PUMPKIN)));
        assert!(!worn_as_armor(&ItemStack::new(1, &Item::CREEPER_HEAD)));
        assert_eq!(SkullType::of_item("creeper_head"), Some(SkullType::Creeper));
        assert_eq!(SkullType::of_item("player_head"), Some(SkullType::Player));
        assert_eq!(SkullType::of_item("stone"), None);
    }
}
