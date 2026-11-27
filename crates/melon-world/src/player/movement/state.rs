//! Movement states: one per `travel` branch, picked by priority each tick.
//!
//! The built-ins mirror `LivingEntity.travel` (flight, water, lava, elytra,
//! land). A mod adds its own (a dodge, a wall run) with
//! [`PlayerState::register_move_state`]; the first active state by priority
//! moves the player. A mod state has a [`MoveState::server_id`] and runs
//! only once the server allows it (`melon:movement`); its start is
//! reported back for the server's grace.

use glam::DVec3;

use super::super::block_effects::FluidHeights;
use super::super::{MoveInput, MoveMode, PlayerState};
use super::rotate_input;
use crate::fluid::FluidKind;
use crate::level::Level;

/// Where the built-ins sit; a custom state picks a number between them.
pub mod priority {
    pub const FLY: i32 = 400;
    pub const FLUID: i32 = 300;
    pub const GLIDE: i32 = 200;
    /// The fallback: always active.
    pub const LAND: i32 = 0;
}

/// What a state sees of this tick.
pub struct Travel<'a> {
    pub level: &'a Level,
    pub input: MoveInput,
    /// Last tick's jump key, for rising edges.
    pub was_jumping: bool,
    /// `LocalPlayer.modifyInput`: the damped, slowed key vector.
    pub forward: f32,
    pub strafe: f32,
    /// Sine and cosine of vanilla's yaw.
    sin: f32,
    cos: f32,
    heights: FluidHeights,
}

impl<'a> Travel<'a> {
    pub(in super::super) fn new(
        player: &PlayerState,
        level: &'a Level,
        input: MoveInput,
        was_jumping: bool,
        heights: FluidHeights,
    ) -> Self {
        let (forward, strafe) = player.movement_input(input);
        let (sin, cos) = crate::math::sin_cos_degrees(player.yaw() + 180.0);
        Self {
            level,
            input,
            was_jumping,
            forward,
            strafe,
            sin,
            cos,
            heights,
        }
    }

    /// The fluid the body is in (`travelInFluid`): water wins.
    pub fn fluid(&self) -> Option<FluidKind> {
        self.heights.travel_fluid()
    }

    pub(in super::super) fn heights(&self) -> FluidHeights {
        self.heights
    }

    pub(in super::super) fn sin_cos(&self) -> (f32, f32) {
        (self.sin, self.cos)
    }

    /// `forward`/`strafe` turned by the yaw into a world vector of `speed`.
    pub fn relative(&self, forward: f32, strafe: f32, speed: f32) -> DVec3 {
        rotate_input(forward, strafe, speed, self.sin, self.cos)
    }
}

/// One way of moving.
pub trait MoveState: Send {
    /// What the tick reports while this state moves the player.
    fn mode(&self, player: &PlayerState) -> MoveMode;

    /// Higher goes first; see [`priority`].
    fn priority(&self) -> i32;

    /// Bookkeeping before any state moves (`aiStep`): triggers and
    /// toggles. Runs for every state, by priority.
    fn update(&mut self, _player: &mut PlayerState, _env: &Travel) {}

    /// Whether this state moves the player this tick.
    fn active(&self, player: &PlayerState, env: &Travel) -> bool;

    /// The state reads jump and sneak itself; otherwise `aiStep`'s jump,
    /// sink and swim stroke run first.
    fn own_vertical_input(&self) -> bool {
        false
    }

    /// One tick of movement.
    fn travel(&mut self, player: &mut PlayerState, env: &Travel);

    /// The id a server must allow over `melon:movement`; vanilla's
    /// states need none.
    fn server_id(&self) -> Option<&'static str> {
        None
    }
}

/// A built-in, or a mod state the server allows.
fn allowed(state: &dyn MoveState, player: &PlayerState) -> bool {
    state.server_id().is_none_or(|id| player.move_allowed(id))
}

/// The registered states, by priority.
#[derive(Default)]
pub struct MoveStates(Vec<Box<dyn MoveState>>);

impl MoveStates {
    pub(in super::super) fn vanilla() -> Self {
        let mut states = Self(Vec::new());
        states.add(Box::new(super::fly::Fly));
        states.add(Box::new(super::fluid::Water));
        states.add(Box::new(super::fluid::Lava));
        states.add(Box::new(super::super::glide::Glide));
        states.add(Box::new(super::land::Land));
        states
    }

    /// After the states of the same priority, so earlier ones keep winning.
    pub(in super::super) fn add(&mut self, state: Box<dyn MoveState>) {
        let at = self.0.partition_point(|s| s.priority() >= state.priority());
        self.0.insert(at, state);
    }

    pub(in super::super) fn update(&mut self, player: &mut PlayerState, env: &Travel) {
        for state in &mut self.0 {
            if allowed(state.as_ref(), player) {
                state.update(player, env);
            }
        }
    }

    /// The first active state; the lowest if none is.
    pub(in super::super) fn select(
        &mut self,
        player: &PlayerState,
        env: &Travel,
    ) -> Option<&mut Box<dyn MoveState>> {
        let at = self
            .0
            .iter()
            .position(|s| allowed(s.as_ref(), player) && s.active(player, env))
            .unwrap_or(self.0.len().saturating_sub(1));
        self.0.get_mut(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{Chunk, Section};
    use crate::player::SPRINT_WINDOW_DEFAULT;

    fn floor() -> Level {
        let mut level = Level::default();
        for cx in -2..=2 {
            for cz in -2..=2 {
                let mut sections: Vec<Section> = (0..24).map(|_| Section::empty()).collect();
                for section in &mut sections[..8] {
                    *section = Section::filled(1);
                }
                level.insert(Chunk::new(cx, cz, sections));
            }
        }
        level
    }

    /// A mod's dodge: sneak + strafe on the ground pushes sideways, then
    /// walks for a few ticks with its own mode.
    struct Dodge {
        ticks: u32,
    }

    impl MoveState for Dodge {
        fn mode(&self, _: &PlayerState) -> MoveMode {
            MoveMode::Custom("example:dodge")
        }

        fn priority(&self) -> i32 {
            priority::GLIDE + 50
        }

        fn update(&mut self, player: &mut PlayerState, env: &Travel) {
            self.ticks = self.ticks.saturating_sub(1);
            if self.ticks == 0 && player.on_ground() && env.input.sneak && env.input.strafe != 0.0 {
                self.ticks = 4;
                player.push(env.relative(0.0, -env.input.strafe, 0.8));
            }
        }

        fn active(&self, _: &PlayerState, _: &Travel) -> bool {
            self.ticks > 0
        }

        fn travel(&mut self, player: &mut PlayerState, env: &Travel) {
            player.travel_on_land(env);
        }

        fn server_id(&self) -> Option<&'static str> {
            Some("example:dodge")
        }
    }

    fn settle(player: &mut PlayerState, level: &Level) {
        for _ in 0..40 {
            player.tick(MoveInput::default(), level, 20, SPRINT_WINDOW_DEFAULT);
        }
        assert!(player.on_ground());
    }

    #[test]
    fn a_standing_player_walks() {
        let level = floor();
        let mut player = PlayerState::new(DVec3::new(0.5, 64.0, 0.5));
        settle(&mut player, &level);
        assert_eq!(player.move_mode(), MoveMode::Walk);
    }

    const DODGE: MoveInput = MoveInput {
        forward: 0.0,
        strafe: 1.0,
        jump: false,
        sprint: false,
        sneak: true,
    };

    #[test]
    fn an_allowed_state_takes_over_reports_its_start_and_hands_back() {
        let level = floor();
        let mut player = PlayerState::new(DVec3::new(0.5, 64.0, 0.5));
        player.register_move_state(Box::new(Dodge { ticks: 0 }));
        player.set_allowed_moves(vec!["example:dodge".into()]);
        settle(&mut player, &level);
        let start = player.feet();
        player.tick(DODGE, &level, 20, SPRINT_WINDOW_DEFAULT);
        assert_eq!(player.move_mode(), MoveMode::Custom("example:dodge"));
        assert!((player.feet() - start).length() > 0.3, "{}", player.feet());
        assert_eq!(player.take_started_moves(), ["example:dodge"]);
        player.tick(MoveInput::default(), &level, 20, SPRINT_WINDOW_DEFAULT);
        assert!(
            player.take_started_moves().is_empty(),
            "one start per dodge"
        );
        settle(&mut player, &level);
        assert_eq!(player.move_mode(), MoveMode::Walk);
    }

    #[test]
    fn a_state_the_server_did_not_allow_never_runs() {
        let level = floor();
        let mut player = PlayerState::new(DVec3::new(0.5, 64.0, 0.5));
        player.register_move_state(Box::new(Dodge { ticks: 0 }));
        settle(&mut player, &level);
        player.tick(DODGE, &level, 20, SPRINT_WINDOW_DEFAULT);
        assert_eq!(player.move_mode(), MoveMode::Walk);
        assert!(player.take_started_moves().is_empty());
    }

    #[test]
    fn a_custom_state_slots_in_by_priority() {
        let mut states = MoveStates::vanilla();
        let land = states.0.len() - 1;
        assert_eq!(states.0[land].priority(), priority::LAND);
        states.add(Box::new(Dodge { ticks: 0 }));
        let at = |states: &MoveStates, p| states.0.iter().position(|s| s.priority() == p);
        assert!(at(&states, priority::GLIDE + 50) < at(&states, priority::GLIDE));
        assert!(at(&states, priority::FLY) < at(&states, priority::GLIDE + 50));
    }
}
