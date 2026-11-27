//! `Display` entities (item, block and text displays).

use std::cell::Cell;

use glam::{EulerRot, Mat4, Quat, Vec3};
use pumpkin_data::item_stack::ItemStack;

use super::kind::EntityKind;
use crate::component::Component;
use crate::entity::wire::DataValue;
use crate::item_model::KeyedStack;

/// `Display.DATA_*`: the entity data indices after `Entity`'s 0-7.
mod index {
    pub const START_DELTA: u8 = 8;
    pub const DURATION: u8 = 9;
    pub const TELEPORT_DURATION: u8 = 10;
    pub const TRANSLATION: u8 = 11;
    pub const SCALE: u8 = 12;
    pub const LEFT_ROTATION: u8 = 13;
    pub const RIGHT_ROTATION: u8 = 14;
    pub const BILLBOARD: u8 = 15;
    pub const BRIGHTNESS: u8 = 16;
    pub const VIEW_RANGE: u8 = 17;
    pub const SHADOW_RADIUS: u8 = 18;
    pub const SHADOW_STRENGTH: u8 = 19;
    pub const WIDTH: u8 = 20;
    pub const HEIGHT: u8 = 21;
    pub const GLOW_COLOR: u8 = 22;
    /// Block state, item stack or text, by kind.
    pub const CONTENT: u8 = 23;
    /// The item's display context, or a text's line width.
    pub const CONTEXT_OR_LINE_WIDTH: u8 = 24;
    pub const BACKGROUND: u8 = 25;
    pub const OPACITY: u8 = 26;
    pub const FLAGS: u8 = 27;
}

/// `Transformation`: translation, left rotation, scale, right rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub left: Quat,
    pub scale: Vec3,
    pub right: Quat,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        translation: Vec3::ZERO,
        left: Quat::IDENTITY,
        scale: Vec3::ONE,
        right: Quat::IDENTITY,
    };

    /// `translate * left * scale * right`.
    pub fn matrix(self) -> Mat4 {
        Mat4::from_translation(self.translation)
            * Mat4::from_quat(self.left)
            * Mat4::from_scale(self.scale)
            * Mat4::from_quat(self.right)
    }

    /// `Transformation.slerp`.
    pub fn slerp(self, to: Transform, progress: f32) -> Transform {
        Transform {
            translation: self.translation.lerp(to.translation, progress),
            left: self.left.slerp(to.left, progress),
            scale: self.scale.lerp(to.scale, progress),
            right: self.right.slerp(to.right, progress),
        }
    }
}

/// What a display's entity data says, and where its transformation is in its interpolation.
#[derive(Clone)]
pub struct DisplayState {
    from: Transform,
    to: Transform,
    /// What the data says now, taken in by the next tick.
    pending: Transform,
    /// A change of the transformation came in since the last tick.
    dirty_transform: bool,
    dirty_start: bool,
    dirty_duration: bool,
    /// Opacity and background changed since the last tick.
    dirty_style: bool,
    /// `TextRenderState`'s interpolators: `(opacity, background)` from and to.
    style_from: (i32, i32),
    style_to: (i32, i32),
    /// Data indices 8 and 9.
    start_delta: i32,
    duration: i32,
    /// `interpolationStartClientTick` (the entity's own tick count).
    start_tick: i32,
    /// `lastProgress`: where the last frame left the interpolation.
    last_progress: Cell<f32>,
    /// A transformation has been shown already.
    shown: bool,
    style_shown: bool,
    /// `BillboardConstraints` by id: fixed, vertical, horizontal, center.
    pub billboard: u8,
    /// Packed block and sky light, or `-1` for the world's.
    pub brightness: i32,
    pub view_range: f32,
    pub width: f32,
    pub height: f32,
    /// `glow_color_override`, `-1` for none.
    pub glow_color: i32,
    pub item: KeyedStack,
    /// `ItemDisplayContext` by id.
    pub item_context: u8,
    pub block_state: u16,
    pub text: Component,
    pub line_width: i32,
    /// ARGB.
    pub background: i32,
    /// Signed, `-1` is fully opaque.
    pub opacity: i8,
    pub flags: u8,
    /// Counts the changes of what a text display shows, for its mesh.
    pub revision: u32,
    /// `shadow_radius` and `shadow_strength` as the data says now, and
    /// interpolated like the style (`Display.RenderState`): from and to.
    shadow: (f32, f32),
    shadow_from: (f32, f32),
    shadow_to: (f32, f32),
    dirty_shadow: bool,
}

impl std::fmt::Debug for DisplayState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DisplayState({:?})", self.to)
    }
}

impl Default for DisplayState {
    fn default() -> Self {
        Self {
            from: Transform::IDENTITY,
            to: Transform::IDENTITY,
            pending: Transform::IDENTITY,
            dirty_transform: false,
            dirty_start: false,
            dirty_duration: false,
            dirty_style: true,
            style_from: (255, 0x4000_0000),
            style_to: (255, 0x4000_0000),
            start_delta: 0,
            duration: 0,
            start_tick: i32::MIN,
            last_progress: Cell::new(0.0),
            shown: false,
            style_shown: false,
            billboard: 0,
            brightness: -1,
            view_range: 1.0,
            width: 0.0,
            height: 0.0,
            glow_color: -1,
            item: KeyedStack::new(ItemStack::EMPTY.clone()),
            item_context: 0,
            block_state: 0,
            text: Component::empty(),
            line_width: 200,
            background: 0x4000_0000,
            opacity: -1,
            flags: 0,
            revision: 0,
            shadow: (0.0, 1.0),
            shadow_from: (0.0, 1.0),
            shadow_to: (0.0, 1.0),
            dirty_shadow: false,
        }
    }
}

/// `TextDisplay.FLAG_*`.
pub const FLAG_SHADOW: u8 = 1;
pub const FLAG_SEE_THROUGH: u8 = 2;
pub const FLAG_DEFAULT_BACKGROUND: u8 = 4;
pub const FLAG_ALIGN_LEFT: u8 = 8;
pub const FLAG_ALIGN_RIGHT: u8 = 16;

impl DisplayState {
    /// Takes in one synced value of a display of `kind`.
    pub fn apply(&mut self, kind: EntityKind, i: u8, value: &DataValue) {
        let int = || value.int();
        if i >= index::CONTENT {
            self.revision = self.revision.wrapping_add(1);
        }
        match i {
            index::START_DELTA => {
                if let Some(v) = int() {
                    self.start_delta = v;
                    self.dirty_start = true;
                }
            }
            index::DURATION => {
                if let Some(v) = int() {
                    self.duration = v;
                    self.dirty_duration = true;
                }
            }
            index::TELEPORT_DURATION => {}
            index::TRANSLATION => {
                if let Some([x, y, z]) = value.vec3() {
                    self.pending.translation = Vec3::new(x, y, z);
                    self.dirty_transform = true;
                }
            }
            index::SCALE => {
                if let Some([x, y, z]) = value.vec3() {
                    self.pending.scale = Vec3::new(x, y, z);
                    self.dirty_transform = true;
                }
            }
            index::LEFT_ROTATION => {
                if let Some([x, y, z, w]) = value.quat() {
                    self.pending.left = quat(x, y, z, w);
                    self.dirty_transform = true;
                }
            }
            index::RIGHT_ROTATION => {
                if let Some([x, y, z, w]) = value.quat() {
                    self.pending.right = quat(x, y, z, w);
                    self.dirty_transform = true;
                }
            }
            index::BILLBOARD => {
                if let Some(b) = value.byte() {
                    self.billboard = if b <= 3 { b } else { 0 };
                }
            }
            index::BRIGHTNESS => {
                if let Some(v) = int() {
                    self.brightness = v;
                }
            }
            index::SHADOW_RADIUS => {
                if let Some(v) = value.float() {
                    self.shadow.0 = v;
                    self.dirty_shadow = true;
                }
            }
            index::SHADOW_STRENGTH => {
                if let Some(v) = value.float() {
                    self.shadow.1 = v;
                    self.dirty_shadow = true;
                }
            }
            index::VIEW_RANGE => {
                if let Some(v) = value.float() {
                    self.view_range = v;
                }
            }
            index::WIDTH => {
                if let Some(v) = value.float() {
                    self.width = v;
                }
            }
            index::HEIGHT => {
                if let Some(v) = value.float() {
                    self.height = v;
                }
            }
            index::GLOW_COLOR => {
                if let Some(v) = int() {
                    self.glow_color = v;
                }
            }
            index::CONTENT => match (kind, value) {
                (EntityKind::ItemDisplay, DataValue::Item(stack)) => {
                    self.item.set((**stack).clone());
                }
                (EntityKind::BlockDisplay, _) => {
                    self.block_state = int().and_then(|s| u16::try_from(s).ok()).unwrap_or(0);
                }
                (EntityKind::TextDisplay, DataValue::Text(text)) => self.text = (**text).clone(),
                _ => {}
            },
            index::CONTEXT_OR_LINE_WIDTH => match kind {
                EntityKind::ItemDisplay => {
                    if let Some(v) = value.byte() {
                        self.item_context = v;
                    }
                }
                EntityKind::TextDisplay => {
                    if let Some(v) = int() {
                        self.line_width = v;
                    }
                }
                _ => {}
            },
            index::BACKGROUND if kind == EntityKind::TextDisplay => {
                if let Some(v) = int() {
                    self.background = v;
                    self.dirty_style = true;
                }
            }
            index::OPACITY if kind == EntityKind::TextDisplay => {
                if let Some(v) = value.byte() {
                    self.opacity = v as i8;
                    self.dirty_style = true;
                }
            }
            index::FLAGS if kind == EntityKind::TextDisplay => {
                if let Some(v) = value.byte() {
                    self.flags = v;
                }
            }
            _ => {}
        }
    }

    /// `Display.tick`, on the client: takes in what changed.
    pub fn tick(&mut self, tick: i32) {
        if std::mem::take(&mut self.dirty_start) {
            self.start_tick = tick.wrapping_add(self.start_delta);
        }
        std::mem::take(&mut self.dirty_duration);
        if std::mem::take(&mut self.dirty_transform) {
            let interpolate = self.duration != 0 && self.shown;
            self.from = if interpolate {
                self.value(self.last_progress.get())
            } else {
                self.pending
            };
            self.to = self.pending;
            self.shown = true;
        }
        if std::mem::take(&mut self.dirty_shadow) {
            self.shadow_from = if self.duration != 0 && self.style_shown {
                self.shadow_value(self.last_progress.get())
            } else {
                self.shadow
            };
            self.shadow_to = self.shadow;
        }
        if std::mem::take(&mut self.dirty_style) {
            let now = (i32::from(self.opacity as u8), self.background);
            self.style_from = if self.duration != 0 && self.style_shown {
                self.style_value(self.last_progress.get())
            } else {
                now
            };
            self.style_to = now;
            self.style_shown = true;
        }
    }

    /// The text opacity and background colour at `progress`
    pub fn style_value(&self, progress: f32) -> (i32, i32) {
        if progress >= 1.0 {
            return self.style_to;
        }
        let lerp = |a: i32, b: i32| (a as f32 + progress * (b - a) as f32).floor() as i32;
        let (from, to) = (self.style_from.1 as u32, self.style_to.1 as u32);
        let mut bg = 0u32;
        for shift in [24, 16, 8, 0] {
            let c = lerp(
                ((from >> shift) & 0xFF) as i32,
                ((to >> shift) & 0xFF) as i32,
            );
            bg |= (c as u32 & 0xFF) << shift;
        }
        (lerp(self.style_from.0, self.style_to.0), bg as i32)
    }

    /// The opacity and background to draw now.
    pub fn style(&self, tick: i32, partial: f32) -> (u8, u32) {
        let (o, b) = self.style_value(self.progress(tick, partial));
        (o as u8, b as u32)
    }

    fn shadow_value(&self, progress: f32) -> (f32, f32) {
        let lerp = |a: f32, b: f32| a + progress * (b - a);
        (
            lerp(self.shadow_from.0, self.shadow_to.0),
            lerp(self.shadow_from.1, self.shadow_to.1),
        )
    }

    /// `DisplayRenderer.getShadowRadius` and `getShadowStrength`.
    pub fn shadow(&self, tick: i32, partial: f32) -> (f32, f32) {
        self.shadow_value(self.progress(tick, partial))
    }

    /// `calculateInterpolationProgress`.
    pub fn progress(&self, tick: i32, partial: f32) -> f32 {
        if self.duration <= 0 {
            self.last_progress.set(1.0);
            return 1.0;
        }
        let elapsed = tick.wrapping_sub(self.start_tick) as f32 + partial;
        let progress = (elapsed / self.duration as f32).clamp(0.0, 1.0);
        self.last_progress.set(progress);
        progress
    }

    /// The transformation at `progress` of the way to the target.
    pub fn value(&self, progress: f32) -> Transform {
        if progress >= 1.0 {
            self.to
        } else {
            self.from.slerp(self.to, progress)
        }
    }

    /// The transformation to draw now.
    pub fn transform(&self, tick: i32, partial: f32) -> Transform {
        self.value(self.progress(tick, partial))
    }

    /// The sky and block light of a brightness override, `None` for the world's light.
    pub fn light_override(&self) -> Option<(u8, u8)> {
        (self.brightness != -1).then(|| {
            let packed = self.brightness as u32;
            (((packed >> 4) & 15) as u8, ((packed >> 20) & 15) as u8)
        })
    }
}

fn quat(x: f32, y: f32, z: f32, w: f32) -> Quat {
    let q = Quat::from_xyzw(x, y, z, w);
    if q.length_squared() > 1e-12 {
        q.normalize()
    } else {
        Quat::IDENTITY
    }
}

/// The camera's yaw and pitch in degrees, from where it looks.
pub fn camera_angles(forward: Vec3) -> (f32, f32) {
    let yaw = (-forward.x).atan2(forward.z).to_degrees();
    let pitch = (-forward.y).clamp(-1.0, 1.0).asin().to_degrees();
    (yaw, pitch)
}

/// `DisplayRenderer.calculateOrientation`.
pub fn orientation(billboard: u8, entity: (f32, f32), camera: (f32, f32)) -> Quat {
    let (yaw, pitch) = match billboard {
        2 => (-entity.0, -camera.1),
        1 => (-(camera.0 - 180.0), entity.1),
        3 => (-(camera.0 - 180.0), -camera.1),
        _ => (-entity.0, entity.1),
    };
    Quat::from_euler(EulerRot::YXZ, yaw.to_radians(), pitch.to_radians(), 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn move_by(state: &mut DisplayState, x: f32) {
        state.apply(
            EntityKind::ItemDisplay,
            index::TRANSLATION,
            &DataValue::Vec3([x, 0.0, 0.0]),
        );
    }

    #[test]
    fn a_change_without_a_duration_applies_at_once() {
        let mut state = DisplayState::default();
        move_by(&mut state, 4.0);
        state.tick(1);
        assert_eq!(state.transform(1, 0.5).translation.x, 4.0);
    }

    #[test]
    fn a_change_with_a_duration_eases_in_from_where_it_is() {
        let mut state = DisplayState::default();
        move_by(&mut state, 4.0);
        state.tick(1);
        state.apply(
            EntityKind::ItemDisplay,
            index::DURATION,
            &DataValue::Int(10),
        );
        state.apply(
            EntityKind::ItemDisplay,
            index::START_DELTA,
            &DataValue::Int(0),
        );
        move_by(&mut state, 14.0);
        state.tick(100);
        assert_eq!(state.transform(100, 0.0).translation.x, 4.0);
        let half = state.transform(105, 0.0).translation.x;
        assert!((half - 9.0).abs() < 1e-4, "{half}");
        assert_eq!(state.transform(110, 0.0).translation.x, 14.0);
        state.progress(105, 0.0);
        state.apply(
            EntityKind::ItemDisplay,
            index::START_DELTA,
            &DataValue::Int(0),
        );
        move_by(&mut state, 0.0);
        state.tick(105);
        assert!((state.transform(105, 0.0).translation.x - 9.0).abs() < 1e-4);
    }

    #[test]
    fn the_matrix_applies_translation_rotation_scale_in_order() {
        let transform = Transform {
            translation: Vec3::new(1.0, 0.0, 0.0),
            left: Quat::IDENTITY,
            scale: Vec3::splat(2.0),
            right: Quat::IDENTITY,
        };
        let point = transform
            .matrix()
            .transform_point3(Vec3::new(1.0, 0.0, 0.0));
        assert!((point - Vec3::new(3.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn brightness_unpacks_block_and_sky() {
        let mut state = DisplayState::default();
        assert_eq!(state.light_override(), None);
        state.brightness = (7 << 4) | (12 << 20);
        assert_eq!(state.light_override(), Some((7, 12)));
    }

    #[test]
    fn a_centered_billboard_faces_the_camera() {
        let facing = orientation(3, (0.0, 0.0), (0.0, 0.0)) * Vec3::Z;
        assert!((facing - Vec3::NEG_Z).length() < 1e-5, "{facing:?}");
        let (yaw, pitch) = camera_angles(Vec3::new(0.0, 0.0, 1.0));
        assert!(yaw.abs() < 1e-4 && pitch.abs() < 1e-4);
        assert!((camera_angles(Vec3::X).0 + 90.0).abs() < 1e-4);
    }
}
