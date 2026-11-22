//! Vanilla `AbstractSliderButton` as `OptionInstance` builds it.

use crate::gui::screen::CursorKind;
use crate::gui::sprite::GuiSprite;
use crate::gui::widgets::{Label, Rect, Ui, Widget};

/// `AbstractSliderButton.HANDLE_WIDTH`.
const HANDLE_W: f32 = 8.0;

#[derive(Clone, Copy)]
pub struct SliderTextures {
    pub track: GuiSprite,
    /// Keyboard focus without value editing. Melon has no keyboard focus yet.
    pub track_highlighted: GuiSprite,
    pub handle: GuiSprite,
    pub handle_highlighted: GuiSprite,
}

pub struct Slider {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Steps from the range's start to its end (`IntRange` max - min).
    steps: u32,
    /// The current step, `0..=steps`.
    step: u32,
    pub label: Label,
    pub hovered: bool,
    dragging: bool,
}

impl Slider {
    pub fn new(steps: u32, step: u32, w: f32, h: f32) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w,
            h,
            steps: steps.max(1),
            step: step.min(steps),
            label: Label::default(),
            hovered: false,
            dragging: false,
        }
    }

    pub fn step(&self) -> u32 {
        self.step
    }

    /// Moves the handle without a drag, for a value changed elsewhere.
    pub fn set_step(&mut self, step: u32) {
        self.step = step.min(self.steps);
    }

    pub fn contains(&self, mx: f32, my: f32) -> bool {
        Widget::contains(self, mx, my)
    }

    pub fn update_hover(&mut self, mx: f32, my: f32) {
        Widget::update_hover(self, mx, my);
    }

    /// `onClick`: starts a drag if `(mx, my)` is on the slider.
    pub fn press(&mut self, mx: f32, my: f32) -> Option<bool> {
        if !self.contains(mx, my) {
            return None;
        }
        self.dragging = true;
        Some(self.set_from_mouse(mx))
    }

    /// `onDrag`: follows the mouse while dragging. Whether the step changed.
    pub fn drag(&mut self, mx: f32) -> bool {
        self.dragging && self.set_from_mouse(mx)
    }

    /// `onRelease`: whether this slider was the one held, and so clicks
    /// now (`AbstractSliderButton` clicks on release, not on press).
    pub fn release(&mut self) -> bool {
        std::mem::take(&mut self.dragging)
    }

    /// `setValueFromMouse`, snapped to the nearest step.
    fn set_from_mouse(&mut self, mx: f32) -> bool {
        let fraction = ((mx - (self.x + HANDLE_W / 2.0)) / (self.w - HANDLE_W)).clamp(0.0, 1.0);
        let step = (fraction * self.steps as f32).round() as u32;
        let changed = step != self.step;
        self.step = step;
        changed
    }

    pub fn draw(&self, ui: &mut Ui) {
        Widget::draw(self, ui);
    }

    /// Draws the slider with `label` in place of its own.
    pub fn draw_labeled(&self, ui: &mut Ui, label: &str) {
        let textures = &ui.assets.slider_textures;
        textures
            .track
            .draw(ui.draw_list, self.x, self.y, self.w, self.h);
        let handle = if self.hovered || self.dragging {
            textures.handle_highlighted
        } else {
            textures.handle
        };
        let fraction = self.step as f32 / self.steps as f32;
        let handle_x = self.x + (fraction * (self.w - HANDLE_W)).floor();
        handle.draw(ui.draw_list, handle_x, self.y, HANDLE_W, self.h);

        let text_w = ui.text_width(label);
        let tx = self.x + (self.w - text_w) / 2.0;
        let ty = self.y + (self.h - 8.0) / 2.0;
        ui.text(label, tx.round(), ty.round(), [1.0; 4]);
    }
}

impl Widget for Slider {
    fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }

    fn set_position(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    fn update_hover(&mut self, mx: f32, my: f32) {
        self.hovered = self.rect().contains(mx, my);
    }

    /// `handleCursor`: an east-west arrow while dragging.
    fn cursor(&self) -> Option<CursorKind> {
        if self.dragging {
            Some(CursorKind::ResizeEw)
        } else if self.hovered {
            Some(CursorKind::Pointer)
        } else {
            None
        }
    }

    fn draw(&self, ui: &mut Ui) {
        let lang = ui.lang;
        self.draw_labeled(ui, self.label.resolve(lang));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drags_snap_to_steps_and_clamp() {
        let mut slider = Slider::new(80, 40, 150.0, 20.0);
        slider.x = 10.0;
        assert_eq!(slider.press(0.0, 0.0), None);
        assert_eq!(slider.press(14.0, 5.0), Some(true));
        assert_eq!(slider.step(), 0);
        assert!(slider.drag(10.0 + 75.0));
        assert_eq!(slider.step(), 40);
        assert!(!slider.drag(10.0 + 75.4));
        assert!(slider.drag(1000.0));
        assert_eq!(slider.step(), 80);
        slider.release();
        assert!(!slider.drag(14.0));
    }
}
