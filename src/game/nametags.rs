//! The name tags over entities (`NameTagFeatureRenderer`).

use glam::{DVec3, Mat4, Vec3};

use super::InWorld;
use super::camera::Camera;
use melon_assets::i18n::Lang;
use melon_base::gfx::quad::DrawList;
use melon_core::math::argb;
use melon_gui::hud::item_slot::ItemPainter;
use melon_gui::text::{self, StyledText};
use melon_protocol::component::Component;
use melon_world::entity::kind::EntityKind;
use melon_world::entity::tracker::ClientEntity;
use melon_world::pick;
use melon_world::scoreboard::Visibility;

/// `EntityRenderer.submitNameTag`'s `scale(0.025)`: blocks a text pixel.
const PIXEL: f32 = 0.025;
/// `LivingEntityRenderer.shouldShowName`: a sneaking entity's name reaches 32 blocks.
const DISCRETE_RANGE: f32 = 32.0;
/// `AvatarRenderer.submitNameDisplay`: tag pixels the ears of `deadmau5` push the tag up.
const EARS_OFFSET: f32 = -10.0;
/// `submitNameDisplay`: 9 * 1.15 after the score, in tag pixels.
const SCORE_GAP: f32 = 9.0 * 1.15;
/// `Options.getBackgroundColor`: black at the text background opacity.
fn background(opacity: f32) -> u32 {
    ((opacity.clamp(0.0, 1.0) * 255.0) as u32) << 24
}

/// `submitNameTag`: the text alpha of a sneaking entity's tag and of a tag behind blocks.
fn dim_alpha(background: f32) -> f32 {
    ((background + 0.75) * 0.5).max(0.5)
}
/// The see-through pass: white at `0x20` alpha.
const BEHIND_ALPHA: f32 = 32.0 / 255.0;

/// The camera of the frame, enough to project a world point to the screen.
#[derive(Clone, Copy, Debug)]
pub struct NameView {
    position: DVec3,
    /// `Camera.yRot` / `xRot`, for billboards.
    look: melon_core::math::euler::Look,
    /// `bobView * view`: the world relative to the camera as it sees it.
    view: Mat4,
    tan_half_fov: f32,
    aspect: f32,
}

impl NameView {
    pub fn of(camera: &Camera, aspect: f32) -> Self {
        Self {
            position: camera.position,
            look: camera.look(),
            view: camera.bob * camera.view,
            tan_half_fov: (camera.fov.to_radians() / 2.0).tan(),
            aspect,
        }
    }

    /// Where `point` falls on a GUI screen of `size`, and how far in front of the camera it is.
    pub fn project(&self, point: DVec3, size: (f32, f32)) -> Option<([f32; 2], f32)> {
        let v = self
            .view
            .transform_point3((point - self.position).as_vec3());
        let depth = -v.z;
        if depth < melon_world::projection::WORLD_Z_NEAR {
            return None;
        }
        let x = v.x / (depth * self.tan_half_fov * self.aspect);
        let y = v.y / (depth * self.tan_half_fov);
        if x.abs() > 1.5 || y.abs() > 1.5 {
            return None;
        }
        Some(([size.0 / 2.0 * (1.0 + x), size.1 / 2.0 * (1.0 - y)], depth))
    }

    /// GUI pixels one pixel of tag text covers at `depth`.
    pub fn text_scale(&self, depth: f32, size: (f32, f32)) -> f32 {
        PIXEL * size.1 / 2.0 / (self.tan_half_fov * depth)
    }
}

impl InWorld {
    /// The shown custom names of the entities in range, as they hang above them.
    pub(super) fn draw_name_tags(
        &self,
        draw_list: &mut DrawList,
        painter: ItemPainter,
        lang: &Lang,
        size: (f32, f32),
        background_opacity: f32,
    ) {
        let Some(view) = self.name_view else {
            return;
        };
        let partial = self.partial_tick();
        let viewer_spectator = self.game_mode.current.is_spectator();
        let looked_at = self.hit_entity().map(|hit| hit.id);
        for entity in self.entities.iter() {
            let feet = entity.pos(partial);
            let dist_sq = feet.distance_squared(view.position) as f32;
            let discrete = entity.appearance.crouching();
            let name = if dist_sq < entity.name_tag_distance * entity.name_tag_distance
                && !(entity.is_living() && discrete && dist_sq >= DISCRETE_RANGE * DISCRETE_RANGE)
            {
                self.name_tag_component(entity, looked_at == Some(entity.id), viewer_spectator)
            } else {
                None
            };
            let score = if dist_sq >= entity.below_name_distance * entity.below_name_distance {
                None
            } else if let Some(mannequin) = &entity.mannequin {
                // `ClientMannequin.belowNameDisplay`: its description, drawn
                // only along with its name (`submitNameTag` skips a tag
                // without the name's attachment).
                mannequin.description.clone().filter(|_| name.is_some())
            } else {
                self.scoreboard
                    .below_name_display(&self.score_holder(entity))
            };
            let height = f64::from(entity.size().1);
            let anchor = feet + DVec3::new(0.0, height + 0.5, 0.0);
            let projected = view.project(anchor, size);
            if melon_core::probe::enabled()
                && entity.uuid != self.uuid
                && (entity.kind == EntityKind::Player || entity.custom_name.is_some())
            {
                let outcome = match (&name, projected) {
                    (Some(_), Some(_)) => "drawn",
                    (Some(_), None) => "off screen",
                    (None, _) => "hidden",
                };
                melon_core::probe::note(
                    "nametag",
                    format!("{} #{} {outcome}", entity.ty.resource_name, entity.id),
                    || self.name_tag_detail(entity, name.as_ref(), dist_sq.sqrt(), lang),
                );
            }
            if name.is_none() && score.is_none() {
                continue;
            }
            let Some((at, depth)) = projected else {
                continue;
            };
            let occluded =
                pick::clip_blocks_for_nametag(&self.level, view.position, anchor).is_some();
            if discrete && occluded {
                continue;
            }
            let dim = discrete || occluded;
            let scale = view.text_scale(depth, size);
            let ears = entity.kind == EntityKind::Player
                && self
                    .players
                    .get(&entity.uuid)
                    .is_some_and(|p| &*p.name == "deadmau5");
            let at = [at[0], at[1] + if ears { EARS_OFFSET * scale } else { 0.0 }];
            if let Some(score) = score {
                let styled = StyledText::from_component(&score, lang);
                if !styled.is_empty() {
                    self.draw_tag(
                        draw_list,
                        painter,
                        &styled,
                        ([at[0], at[1] + SCORE_GAP * scale], scale),
                        dim.then(|| dim_alpha(background_opacity)),
                        background_opacity,
                    );
                }
            }
            if let Some(name) = name {
                let styled = StyledText::from_component(&name, lang);
                if styled.is_empty() {
                    continue;
                }
                self.draw_tag(
                    draw_list,
                    painter,
                    &styled,
                    (at, scale),
                    dim.then(|| dim_alpha(background_opacity)),
                    background_opacity,
                );
            }
        }
    }

    /// The text displays in range (`TextDisplayRenderer`).
    pub(super) fn draw_text_displays(
        &self,
        draw_list: &mut DrawList,
        painter: ItemPainter,
        lang: &Lang,
        size: (f32, f32),
        background_opacity: f32,
    ) {
        use melon_world::entity::display::{
            FLAG_ALIGN_LEFT, FLAG_ALIGN_RIGHT, FLAG_DEFAULT_BACKGROUND, FLAG_SEE_THROUGH,
            FLAG_SHADOW,
        };
        let Some(view) = self.name_view else {
            return;
        };
        let partial = self.partial_tick();
        for entity in self.entities.iter() {
            if entity.kind != EntityKind::TextDisplay {
                continue;
            }
            let Some(state) = entity.display.as_deref() else {
                continue;
            };
            if state.flags & FLAG_SEE_THROUGH == 0 {
                continue;
            }
            let tick = entity.age_in_ticks(0.0) as i32;
            let transform = state.transform(tick, partial);
            let facing = melon_world::entity::display::orientation(
                state.billboard,
                (entity.yaw_degrees(partial), entity.pitch_degrees(partial)),
                (view.look.y_rot, view.look.x_rot),
            );
            let anchor = entity.pos(partial) + (facing * transform.translation).as_dvec3();
            if anchor.distance_squared(view.position) as f32 >= (state.view_range * 64.0).powi(2) {
                continue;
            }
            let Some((at, depth)) = view.project(anchor, size) else {
                continue;
            };
            let full = StyledText::from_component(&state.text, lang);
            if full.is_empty() {
                continue;
            }
            let lines = text::wrap(painter.font, &full, state.line_width.max(1) as f32);
            let widths: Vec<f32> = lines.iter().map(|l| text::width(painter.font, l)).collect();
            let box_w = widths.iter().copied().fold(0.0_f32, f32::max);
            let box_h = lines.len() as f32 * 10.0 - 1.0;
            let up = Vec3::new(view.view.x_axis.y, view.view.y_axis.y, view.view.z_axis.y);
            let middle = anchor
                + (up * (box_h * 0.5 * 0.025 * transform.scale.y.abs().max(1e-3))).as_dvec3();
            let hidden =
                pick::clip_blocks_for_nametag(&self.level, view.position, middle).is_some();
            if melon_core::probe::enabled() {
                melon_core::probe::note("text_display", format!("#{}", entity.id), || {
                    format!(
                        "pos={:?} billboard={} translation={:?} scale={:?} left={:?} right={:?} anchor={anchor:?} hidden={hidden}",
                        entity.pos(partial),
                        state.billboard,
                        transform.translation,
                        transform.scale,
                        transform.left,
                        transform.right,
                    )
                });
            }
            let see_through = state.flags & FLAG_SEE_THROUGH != 0;
            if hidden && !see_through {
                continue;
            }
            let opacity = f32::from(state.opacity as u8) / 255.0;
            let alpha = if hidden { BEHIND_ALPHA } else { opacity };
            let scale =
                view.text_scale(depth, size) * transform.scale.y.abs().max(1e-3) / PIXEL * PIXEL;
            let background = if state.flags & FLAG_DEFAULT_BACKGROUND != 0 {
                background(background_opacity)
            } else {
                state.background as u32
            };
            if background >> 24 != 0 {
                draw_list.push_fill(
                    painter.white,
                    at[0] + (1.0 - box_w / 2.0 - 1.0) * scale,
                    at[1] - (box_h + 1.0) * scale,
                    box_w * scale,
                    box_h * scale,
                    argb(background),
                );
            }
            let start = draw_list.vertices_len();
            let paint = text::TextPaint {
                alpha,
                shadow: state.flags & FLAG_SHADOW != 0,
                obfuscation_seed: (entity.id as u64)
                    ^ self.ticks.wrapping_mul(0x9E37_79B9_7F4A_7C15),
                ..painter.paint(text::colors::WHITE)
            };
            for (i, (line, width)) in lines.iter().zip(&widths).enumerate() {
                let x = if state.flags & FLAG_ALIGN_LEFT != 0 {
                    0.0
                } else if state.flags & FLAG_ALIGN_RIGHT != 0 {
                    box_w - width
                } else {
                    box_w / 2.0 - width / 2.0
                };
                text::draw(
                    draw_list,
                    &paint,
                    line,
                    1.0 - box_w / 2.0 + x,
                    -box_h + 10.0 * i as f32,
                );
            }
            draw_list.scale_since(start, scale);
            draw_list.offset_since(start, at);
        }
    }

    /// `EntityRenderer.getNameTag`, if the renderer's `shouldShowName` lets it show.
    fn name_tag_component(
        &self,
        entity: &ClientEntity,
        looked_at: bool,
        viewer_spectator: bool,
    ) -> Option<Component> {
        if entity.kind == EntityKind::Other {
            return None;
        }
        if matches!(
            entity.kind,
            EntityKind::ItemFrame | EntityKind::GlowItemFrame
        ) {
            let name = entity.item.get_custom_name().filter(|_| looked_at)?;
            return Some(melon_protocol::component::pumpkin::from_pumpkin(name));
        }
        let player = entity.kind == EntityKind::Player;
        if entity.kind == EntityKind::ArmorStand {
            if !entity.appearance.name_visible {
                return None;
            }
        } else {
            let shown = player
                || entity.appearance.name_visible
                || entity.custom_name.is_some() && looked_at;
            if !shown || entity.is_living() && !self.living_shows_name(entity, viewer_spectator) {
                return None;
            }
        }
        let name = if player {
            Component::text(self.players.get(&entity.uuid)?.name.to_string())
        } else {
            match entity.custom_name.as_deref() {
                Some(name) => without_click(name),
                None => Component::translatable(
                    format!("entity.minecraft.{}", entity.ty.resource_name),
                    Vec::new(),
                ),
            }
        };
        Some(
            self.scoreboard
                .format_name(&self.score_holder(entity), name),
        )
    }

    /// What decides an entity's name tag, for the probe.
    fn name_tag_detail(
        &self,
        entity: &ClientEntity,
        name: Option<&Component>,
        dist: f32,
        lang: &Lang,
    ) -> String {
        let holder = self.score_holder(entity);
        let team = self.scoreboard.team_of(&holder).map(|(name, team)| {
            format!(
                "{name:?} name_tags={:?} prefix={:?} suffix={:?}",
                team.look.name_tags, team.look.prefix, team.look.suffix
            )
        });
        let text = name.map(|name| {
            let styled = StyledText::from_component(name, lang);
            let fonts: Vec<&str> = styled
                .spans
                .iter()
                .filter_map(|s| s.style.font.as_deref())
                .collect();
            format!("{:?} fonts={fonts:?}", styled.to_plain())
        });
        format!(
            "text={text:?} dist={dist:.1} range={} holder={holder:?} in_tab={} team={team:?} \
             invisible={} crouching={} name_visible={} custom_name={:?} passengers={} vehicle={:?}",
            entity.name_tag_distance,
            self.players.contains_key(&entity.uuid),
            entity.appearance.invisible(),
            entity.appearance.crouching(),
            entity.appearance.name_visible,
            entity.custom_name,
            entity.passengers.len(),
            entity.vehicle,
        )
    }

    /// `Entity.getScoreboardName`: a player's profile name, else the UUID.
    fn score_holder(&self, entity: &ClientEntity) -> String {
        if entity.kind == EntityKind::Player
            && let Some(player) = self.players.get(&entity.uuid)
        {
            return player.name.to_string();
        }
        entity.uuid.hyphenated().to_string()
    }

    /// `LivingEntityRenderer.shouldShowName` past the sneak range.
    fn living_shows_name(&self, entity: &ClientEntity, viewer_spectator: bool) -> bool {
        if entity.uuid == self.uuid {
            return false;
        }
        let own = self
            .players
            .get(&self.uuid)
            .and_then(|p| self.scoreboard.team_of(&p.name));
        let holder = self.score_holder(entity);
        let theirs = self.scoreboard.team_of(&holder);
        let same = matches!((own, theirs), (Some((a, _)), Some((b, _))) if a == b);
        let spectator = entity.kind == EntityKind::Player
            && self
                .players
                .get(&entity.uuid)
                .is_some_and(|p| p.game_mode.is_spectator());
        let invisible = entity.appearance.invisible() || spectator;
        let visible = viewer_spectator
            || !invisible
            || same && theirs.is_some_and(|(_, team)| team.look.sees_friendly_invisibles());
        let Some((_, team)) = theirs else {
            return visible && entity.passengers.is_empty();
        };
        match team.look.name_tags {
            Visibility::Always => visible,
            Visibility::Never => false,
            Visibility::HideForOtherTeams => {
                if own.is_none() {
                    visible
                } else {
                    same && (team.look.sees_friendly_invisibles() || visible)
                }
            }
            Visibility::HideForOwnTeam => {
                if own.is_none() {
                    visible
                } else {
                    !same && visible
                }
            }
        }
    }

    /// `styled` on its background, `scale` times as big, bottom centre at `at`.
    fn draw_tag(
        &self,
        draw_list: &mut DrawList,
        painter: ItemPainter,
        styled: &StyledText,
        (at, scale): ([f32; 2], f32),
        dim: Option<f32>,
        background_opacity: f32,
    ) {
        let width = text::width(painter.font, styled);
        let (left, top) = (-width / 2.0 - 1.0, -1.0);
        let (box_w, box_h) = (width + 2.0, 10.0);
        draw_list.push_fill(
            painter.white,
            at[0] + left * scale,
            at[1] + top * scale - box_h * scale,
            box_w * scale,
            box_h * scale,
            argb(background(background_opacity)),
        );
        let start = draw_list.vertices_len();
        let paint = text::TextPaint {
            alpha: dim.unwrap_or(1.0),
            shadow: false,
            obfuscation_seed: self.ticks.wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ u64::from(at[0].to_bits()),
            ..painter.paint(text::colors::WHITE)
        };
        text::draw(draw_list, &paint, styled, -width / 2.0, -9.0);
        draw_list.scale_since(start, scale);
        draw_list.offset_since(start, at);
        if melon_core::probe::enabled() {
            melon_core::probe::note("nametag.draw", styled.to_plain(), || {
                format!(
                    "at={at:?} scale={scale:.3} width={width} glyph_vertices={} alpha={:?} background={background_opacity}",
                    draw_list.vertices_len() - start,
                    dim,
                )
            });
        }
    }
}

/// `Entity.removeAction`: the name without its click events.
fn without_click(name: &Component) -> Component {
    let mut out = name.clone();
    out.style.click_event = None;
    out.extra = name.extra.iter().map(without_click).collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> NameView {
        NameView {
            position: DVec3::ZERO,
            look: melon_core::math::euler::Look::new(180.0, 0.0),
            view: Mat4::IDENTITY,
            tan_half_fov: (35.0_f32).to_radians().tan(),
            aspect: 16.0 / 9.0,
        }
    }

    #[test]
    fn a_point_ahead_is_in_the_middle_and_one_behind_is_not_drawn() {
        let size = (320.0, 180.0);
        let (at, depth) = view().project(DVec3::new(0.0, 0.0, -5.0), size).unwrap();
        assert!((at[0] - 160.0).abs() < 1e-3 && (at[1] - 90.0).abs() < 1e-3);
        assert!((depth - 5.0).abs() < 1e-6);
        assert!(view().project(DVec3::new(0.0, 0.0, 5.0), size).is_none());
        let (above, _) = view().project(DVec3::new(0.0, 1.0, -5.0), size).unwrap();
        assert!(above[1] < 90.0);
    }

    /// Points are made relative to the camera in doubles: a million
    /// blocks out a tag sits where it would at the origin.
    #[test]
    fn a_camera_away_from_the_origin_sees_a_point_at_its_real_distance() {
        let eye = DVec3::new(1_000_000.3, 75.0, -1_000_044.7);
        let camera = Camera::new(
            eye,
            melon_core::math::euler::Look::new(20.0, 10.0),
            70.0,
            256.0,
        );
        let camera = NameView::of(&camera, 16.0 / 9.0);
        let size = (320.0, 180.0);
        let forward = melon_core::math::euler::Orientation::new(camera.look).forwards;
        let (at, depth) = camera
            .project(eye + forward.as_dvec3() * 4.0, size)
            .unwrap();
        assert!((at[0] - 160.0).abs() < 1e-2 && (at[1] - 90.0).abs() < 1e-2);
        assert!((depth - 4.0).abs() < 1e-3);
    }

    #[test]
    fn the_text_shrinks_with_distance_like_the_world_quad() {
        let size = (320.0, 180.0);
        let near = view().text_scale(2.0, size);
        let far = view().text_scale(4.0, size);
        assert!((near / far - 2.0).abs() < 1e-5);
    }
}
