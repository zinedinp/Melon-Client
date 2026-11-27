use crate::assets::GuiAssets;
use crate::screen::CursorKind;
use crate::sprite::GuiSprite;
use crate::text::StyledText;
use crate::widgets::{Label, Rect, Ui, Widget};

/// The three button states' sprites (vanilla's are 200x20 nine-slices).
#[derive(Clone, Copy)]
pub struct ButtonTextures {
    pub normal: GuiSprite,
    pub highlighted: GuiSprite,
    pub disabled: GuiSprite,
}

/// `MenuTabBar.MenuTabButton.SPRITES` (`WidgetSprites`): the four tab states.
#[derive(Clone, Copy)]
pub struct TabTextures {
    pub normal: GuiSprite,
    pub highlighted: GuiSprite,
    pub selected: GuiSprite,
    pub selected_highlighted: GuiSprite,
}

impl TabTextures {
    /// `WidgetSprites.get(enabled, focused)` as `MenuTabButton` calls it.
    pub fn get(&self, selected: bool, hovered: bool) -> GuiSprite {
        match (selected, hovered) {
            (true, true) => self.selected_highlighted,
            (true, false) => self.selected,
            (false, true) => self.highlighted,
            (false, false) => self.normal,
        }
    }
}

/// Picks an icon button's sprite out of the GUI assets.
pub type IconSprite = fn(&GuiAssets) -> GuiSprite;

pub struct Button {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: Label,
    /// Centered icon instead of label (`CommonButtons`).
    pub icon: Option<IconSprite>,
    pub hovered: bool,
    pub enabled: bool,
    /// Has the keyboard focus (Tab): drawn highlighted like a hover.
    pub focused: bool,
    /// A label with styles (a server's text) drawn in place of `label`.
    pub styled: Option<StyledText>,
}

impl Button {
    pub fn new(label: impl Into<Label>, x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            label: label.into(),
            icon: None,
            hovered: false,
            enabled: true,
            focused: false,
            styled: None,
        }
    }

    /// A button laid out later, by `resize`.
    pub fn sized(label: impl Into<Label>, w: f32, h: f32) -> Self {
        Self::new(label, 0.0, 0.0, w, h)
    }

    /// `SpriteIconButton`: a square icon. `label` is its tooltip text.
    pub fn icon(label: impl Into<Label>, icon: IconSprite, size: f32) -> Self {
        Self {
            icon: Some(icon),
            ..Self::sized(label, size, size)
        }
    }

    /// Whether a click at `(x, y)` presses this button.
    pub fn clicked(&self, x: f32, y: f32) -> bool {
        self.enabled && self.contains(x, y)
    }

    pub fn contains(&self, mx: f32, my: f32) -> bool {
        Widget::contains(self, mx, my)
    }

    pub fn update_hover(&mut self, mx: f32, my: f32) {
        Widget::update_hover(self, mx, my);
    }

    pub fn draw(&self, ui: &mut Ui) {
        Widget::draw(self, ui);
    }

    /// Draws the button with `label` in place of its own.
    pub fn draw_labeled(&self, ui: &mut Ui, label: &str) {
        let color = if self.enabled {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            [0.62, 0.62, 0.62, 1.0]
        };
        self.draw_labeled_colored(ui, label, color);
    }

    /// [`Self::draw_labeled`] with a chosen text colour.
    pub fn draw_labeled_colored(&self, ui: &mut Ui, label: &str, color: [f32; 4]) {
        if self.draw_frame(ui) {
            return;
        }
        let text_width = ui.text_width(label);
        let tx = self.x + (self.w - text_width) / 2.0;
        let ty = self.y + (self.h - 8.0) / 2.0;
        ui.text(label, tx.round(), ty.round(), color);
    }

    /// [`Self::draw_labeled`] with a styled label, white where it has no colour.
    pub fn draw_styled(&self, ui: &mut Ui, label: &StyledText) {
        if self.draw_frame(ui) {
            return;
        }
        let text_width = ui.styled_width(label);
        let tx = self.x + (self.w - text_width) / 2.0;
        let ty = self.y + (self.h - 8.0) / 2.0;
        ui.styled(label, tx.round(), ty.round(), [1.0; 3]);
    }

    /// The button's sprite, and its icon if it has one; whether it had.
    fn draw_frame(&self, ui: &mut Ui) -> bool {
        let textures = &ui.assets.button_textures;
        let texture = if !self.enabled {
            textures.disabled
        } else if self.hovered || self.focused {
            textures.highlighted
        } else {
            textures.normal
        };
        texture.draw(ui.draw_list, self.x, self.y, self.w, self.h);

        if let Some(icon) = self.icon.map(|icon| icon(ui.assets)) {
            let size = 15.0f32.min(self.w).min(self.h);
            let ix = self.x + (self.w - size) / 2.0;
            let iy = self.y + (self.h - size) / 2.0;
            icon.draw(ui.draw_list, ix.round(), iy.round(), size, size);
            return true;
        }
        false
    }
}

impl Widget for Button {
    fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }

    fn set_position(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    fn update_hover(&mut self, mx: f32, my: f32) {
        self.hovered = self.enabled && self.rect().contains(mx, my);
    }

    fn cursor(&self) -> Option<CursorKind> {
        self.hovered.then_some(CursorKind::Pointer)
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn draw(&self, ui: &mut Ui) {
        if let Some(styled) = &self.styled {
            return self.draw_styled(ui, styled);
        }
        let lang = ui.lang;
        self.draw_labeled(ui, self.label.resolve(lang));
    }
}
