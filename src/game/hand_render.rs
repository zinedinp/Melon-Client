//! The local player's hands as drawn.

use glam::{Mat4, Vec3};

use super::InWorld;
use melon_world::entity::humanoid::{ArmPose, SwingPose};
use melon_world::entity::player::{Arm, PlayerModelPart};
use melon_world::player::ArmPoses;
use melon_world::player::first_person;
use melon_world::player::hands::{Hand, UseAnimation};

/// One thing a first-person hand draws.
pub(super) enum HandDraw<'a> {
    /// The bare arm, `matrix` placing it where `renderHand` draws it.
    Arm {
        arm: Arm,
        matrix: Mat4,
        sleeve: bool,
    },
    /// An item at `matrix`, in the hand of `arm`, raised (`using`) or not.
    Item {
        stack: &'a pumpkin_data::item_stack::ItemStack,
        arm: Arm,
        matrix: Mat4,
        using: melon_world::item_model::HeldUse,
    },
}

impl InWorld {
    /// `AvatarRenderer.getArmPose` for both arms.
    pub(super) fn arm_poses(&self, main_arm: Arm) -> ArmPoses {
        let pose = |hand: Hand| {
            if self.hand_stack(hand).is_empty() {
                ArmPose::Empty
            } else if let Some(using) = self
                .hands
                .using_in(hand)
                .filter(|using| using.remaining > 0)
            {
                using.item_use.animation.arm_pose()
            } else {
                ArmPose::Item
            }
        };
        ArmPoses::new(main_arm, pose(Hand::Main), pose(Hand::Off))
    }

    /// The swing under way, `partial` of a tick on.
    pub(super) fn swing_pose(&self, main_arm: Arm, partial: f32) -> Option<SwingPose> {
        let (hand, swing) = self.hands.swing.current()?;
        Some(SwingPose {
            arm: hand.arm(main_arm),
            animation: self.hands.swing.animation(partial),
            whack: swing.whack,
        })
    }

    /// What the hands show in first person from the camera at `eye`.
    pub(super) fn first_person_hands(
        &self,
        eye: Vec3,
        main_arm: Arm,
        shown_parts: melon_world::entity::player::ShownParts,
        partial: f32,
    ) -> Vec<HandDraw<'_>> {
        let space = self.player.hand_space(eye);
        let swing = self.hands.swing.current();
        let attack = self.hands.swing.animation(partial);
        let mut draws = Vec::with_capacity(2);
        for hand in Hand::ALL {
            let arm = hand.arm(main_arm);
            let stack = self.hands.shown(hand);
            let swinging = swing.filter(|(swung, _)| *swung == hand);
            let hand_attack = if swinging.is_some() { attack } else { 0.0 };
            let lowered = self.hands.lowered(hand, partial);
            if stack.is_empty() {
                if hand == Hand::Main {
                    let sleeve = match arm {
                        Arm::Right => PlayerModelPart::RightSleeve,
                        Arm::Left => PlayerModelPart::LeftSleeve,
                    };
                    draws.push(HandDraw::Arm {
                        arm,
                        matrix: self.player.first_person_arm_transform(
                            eye,
                            arm,
                            lowered,
                            hand_attack,
                        ),
                        sleeve: shown_parts.shows(sleeve),
                    });
                }
                continue;
            }
            let using = self.hands.using_in(hand);
            let local = match using.filter(|using| using.remaining > 0) {
                Some(using) => match using.item_use.animation {
                    UseAnimation::Eat | UseAnimation::Drink => {
                        first_person::eat(arm, using.remaining, partial, using.item_use.duration)
                            * first_person::item_arm(arm, lowered)
                    }
                    UseAnimation::Bow => {
                        first_person::item_arm(arm, lowered)
                            * first_person::draw_bow(
                                arm,
                                using.remaining,
                                partial,
                                using.item_use.duration,
                            )
                    }
                    UseAnimation::Trident => {
                        first_person::item_arm(arm, lowered)
                            * first_person::draw_trident(
                                arm,
                                using.remaining,
                                partial,
                                using.item_use.duration,
                            )
                    }
                    UseAnimation::Block | UseAnimation::None => {
                        first_person::item_arm(arm, lowered)
                    }
                },
                None => {
                    let base = first_person::item_arm(arm, lowered);
                    match swinging {
                        Some((_, swing)) if swing.whack => {
                            base * first_person::swing_item(arm, hand_attack)
                        }
                        _ => base,
                    }
                }
            };
            draws.push(HandDraw::Item {
                stack,
                arm,
                matrix: space * local,
                using: self.held_use(hand, stack, using.is_some()),
            });
        }
        draws
    }
}
