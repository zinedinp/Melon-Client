//! What every widget shares.

pub mod button;
pub mod checkbox;
pub mod dialog;
pub mod edit_box;
pub mod layout;
pub mod multiline_edit;
pub mod options_list;
pub mod row_icon;
pub mod scroll_list;
pub mod set;
pub mod slider;
pub mod tab_bar;
pub mod text_input;
pub mod tooltip;

use crate::gui::assets::GuiAssets;
use crate::gui::screen::CursorKind;
use crate::gui::text::font::Font;
use crate::i18n::Lang;
use crate::renderer::TextureId;
use crate::renderer::quad::DrawList;

/// A widget's area in GUI pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// `AbstractWidget.isMouseOver`: the right and bottom edges are outside.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// A widget's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Label {
    Key(&'static str),
    Text(String),
}

impl Label {
    /// The text in the current language.
    pub fn resolve<'a>(&'a self, lang: &'a Lang) -> &'a str {
        match self {
            Label::Key(key) => lang.get(key),
            Label::Text(text) => text,
        }
    }
}

impl Default for Label {
    fn default() -> Self {
        Label::Text(String::new())
    }
}

/// A `&'static str` is a translation key: widgets name their text by key.
impl From<&'static str> for Label {
    fn from(key: &'static str) -> Self {
        Label::Key(key)
    }
}

impl From<String> for Label {
    fn from(text: String) -> Self {
        Label::Text(text)
    }
}

/// The painter screens and widgets draw with.
pub struct Ui<'a> {
    pub draw_list: &'a mut DrawList,
    pub assets: &'a GuiAssets,
    pub lang: &'a Lang,
    /// The GUI's size in GUI pixels.
    pub width: f32,
    pub height: f32,
    /// What draws items, in a world (whose icons are prepared for the
    /// stacks a screen shows, `Screen::visit_stacks`); `None` in the menus.
    pub items: Option<crate::gui::hud::item_slot::ItemPainter<'a>>,
}

impl<'a> Ui<'a> {
    pub fn new(
        draw_list: &'a mut DrawList,
        assets: &'a GuiAssets,
        lang: &'a Lang,
        size: (f32, f32),
    ) -> Self {
        Self {
            draw_list,
            assets,
            lang,
            width: size.0,
            height: size.1,
            items: None,
        }
    }

    /// With items drawn by `painter`.
    pub fn with_items(mut self, painter: crate::gui::hud::item_slot::ItemPainter<'a>) -> Self {
        self.items = Some(painter);
        self
    }

    pub fn font(&self) -> &'a Font {
        &self.assets.font
    }

    pub fn pages(&self) -> &'a [TextureId] {
        &self.assets.font_textures
    }

    /// The text for a translation key.
    pub fn tr(&self, key: &'static str) -> &'a str {
        self.lang.get(key)
    }

    pub fn text_width(&self, text: &str) -> f32 {
        self.assets.font.text_width(text)
    }

    /// Text with the drop shadow, top-left at `(x, y)`. Returns its width.
    pub fn text(&mut self, text: &str, x: f32, y: f32, color: [f32; 4]) -> f32 {
        self.assets.font.push_text(
            self.draw_list,
            &self.assets.font_textures,
            text,
            x,
            y,
            color,
        )
    }

    /// [`Self::text`] scaled about its top-left (launcher UUID / play-time).
    pub fn text_scaled(&mut self, text: &str, x: f32, y: f32, color: [f32; 4], scale: f32) -> f32 {
        if (scale - 1.0).abs() < 0.01 {
            return self.text(text, x, y, color);
        }
        let start = self.draw_list.vertices_len();
        let w = self.assets.font.push_text(
            self.draw_list,
            &self.assets.font_textures,
            text,
            x / scale,
            y / scale,
            color,
        );
        self.draw_list.scale_since(start, scale);
        w * scale
    }

    /// `text` cut with `...` so its width is at most `max_w`.
    pub fn ellipsize(&self, text: &str, max_w: f32) -> String {
        if self.text_width(text) <= max_w {
            return text.to_owned();
        }
        let dots = "...";
        let dw = self.text_width(dots);
        if max_w <= dw {
            return dots.to_owned();
        }
        let mut out = String::new();
        for c in text.chars() {
            let next = format!("{out}{c}");
            if self.text_width(&next) + dw > max_w {
                break;
            }
            out.push(c);
        }
        out.push_str(dots);
        out
    }

    /// A 1px framed panel (launcher columns).
    pub fn panel(&mut self, rect: Rect, fill: [f32; 4], edge: [f32; 4]) {
        self.fill(rect, fill);
        self.fill(Rect::new(rect.x, rect.y, rect.w, 1.0), edge);
        self.fill(Rect::new(rect.x, rect.bottom() - 1.0, rect.w, 1.0), edge);
        self.fill(Rect::new(rect.x, rect.y, 1.0, rect.h), edge);
        self.fill(Rect::new(rect.right() - 1.0, rect.y, 1.0, rect.h), edge);
    }

    /// The width of styled text: bold glyphs are a pixel wider.
    pub fn styled_width(&self, text: &crate::gui::text::StyledText) -> f32 {
        crate::gui::text::width(&self.assets.font, text)
    }

    /// A list's section header, as vanilla's `OptionsList.HeaderEntry` draws it.
    pub fn section_header(&mut self, text: &str, y: f32) {
        let style = crate::gui::text::TextStyle {
            bold: true,
            underlined: true,
            ..Default::default()
        };
        let mut styled = crate::gui::text::StyledText::default();
        styled.push(text, &style);
        let padded = self.styled_width(&styled) + 8.0;
        let x = (self.width / 2.0).floor() - (padded / 2.0).floor() + 4.0;
        self.styled(&styled, x, y, crate::gui::text::colors::WHITE);
    }

    /// `GuiGraphics.centeredText`: centred on `center_x`, on whole pixels.
    pub fn centered(&mut self, text: &str, center_x: f32, y: f32, color: [f32; 4]) {
        self.assets.font.push_centered(
            self.draw_list,
            &self.assets.font_textures,
            text,
            center_x,
            y,
            color,
        );
    }

    /// A white title centred across the screen.
    pub fn title(&mut self, text: &str, y: f32) {
        self.title_colored(text, y, [1.0; 4]);
    }

    /// A line centred across the screen.
    pub fn title_colored(&mut self, text: &str, y: f32, color: [f32; 4]) {
        let center = self.width / 2.0;
        self.centered(text, center, y, color);
    }

    /// A widget's `Tooltip`.
    pub fn widget_tooltip(&mut self, text: &str, mouse: (f32, f32), widget: Rect) {
        let lines: Vec<_> = self
            .assets
            .font
            .wrap(text, tooltip::MAX_WIDTH)
            .into_iter()
            .map(|line| crate::gui::text::StyledText::plain(&line))
            .collect();
        let paint = crate::gui::text::TextPaint {
            font: &self.assets.font,
            pages: &self.assets.font_textures,
            white: self.assets.white,
            color: crate::gui::text::colors::WHITE,
            alpha: 1.0,
            shadow: true,
            obfuscation_seed: 0,
        };
        let at = tooltip::by_widget(
            mouse,
            widget,
            tooltip::size(&paint, &lines),
            (self.width, self.height),
        );
        tooltip::draw(
            self.draw_list,
            self.assets.tooltip.as_ref(),
            &paint,
            &lines,
            at,
        );
    }

    fn paint(&self, color: [f32; 3]) -> crate::gui::text::TextPaint<'a> {
        crate::gui::text::TextPaint {
            font: &self.assets.font,
            pages: &self.assets.font_textures,
            white: self.assets.white,
            color,
            alpha: 1.0,
            shadow: true,
            obfuscation_seed: 0,
        }
    }

    /// Styled text, the runs without their own colour in `color`.
    pub fn styled(
        &mut self,
        text: &crate::gui::text::StyledText,
        x: f32,
        y: f32,
        color: [f32; 3],
    ) -> f32 {
        let paint = self.paint(color);
        crate::gui::text::draw(self.draw_list, &paint, text, x, y)
    }

    /// `Font.split`: styled text broken into lines at most `max_width` wide.
    pub fn wrap_styled(
        &self,
        text: &crate::gui::text::StyledText,
        max_width: f32,
    ) -> Vec<crate::gui::text::StyledText> {
        crate::gui::text::wrap(&self.assets.font, text, max_width)
    }

    /// `GuiGraphics.setTooltipForNextFrame` with lines.
    pub fn mouse_tooltip(&mut self, lines: &[crate::gui::text::StyledText], mouse: (f32, f32)) {
        let paint = self.paint(crate::gui::text::colors::WHITE);
        let at = tooltip::beside_mouse(
            mouse,
            tooltip::size(&paint, lines),
            (self.width, self.height),
        );
        tooltip::draw(
            self.draw_list,
            self.assets.tooltip.as_ref(),
            &paint,
            lines,
            at,
        );
    }

    /// A sprite over a rectangle, scaled as its `.mcmeta` says.
    pub fn sprite(&mut self, sprite: impl Into<crate::gui::sprite::GuiSprite>, rect: Rect) {
        sprite
            .into()
            .draw(self.draw_list, rect.x, rect.y, rect.w, rect.h);
    }

    /// `Screen.extractBackground`: the dimmed menu background.
    pub fn menu_background(&mut self) {
        crate::gui::background::push_menu_overlay(
            self.draw_list,
            self.assets.white,
            self.width,
            self.height,
        );
    }

    /// `AbstractScrollArea.extractListBackground`/`extractListSeparators`
    pub fn list_background(&mut self, top: f32, bottom: f32) {
        crate::gui::background::push_list_background(
            self.draw_list,
            self.assets,
            top,
            bottom,
            self.width,
        );
    }

    /// A solid colour over a rectangle.
    pub fn fill(&mut self, rect: Rect, color: [f32; 4]) {
        self.draw_list.push_rect(
            self.assets.white,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            [0.0, 0.0, 1.0, 1.0],
            color,
        );
    }

    /// Clips what follows to `rect`, until [`Ui::unclip`].
    pub fn clip(&mut self, rect: Rect) {
        self.draw_list
            .set_scissor(Some([rect.x, rect.y, rect.w, rect.h]));
    }

    pub fn unclip(&mut self) {
        self.draw_list.set_scissor(None);
    }
}

/// A widget a screen lays out, hovers, draws and asks for its cursor.
pub trait Widget {
    fn rect(&self) -> Rect;
    fn set_position(&mut self, x: f32, y: f32);
    fn update_hover(&mut self, x: f32, y: f32);
    /// The cursor this widget asks for at the last hover position, if any.
    fn cursor(&self) -> Option<CursorKind>;
    fn draw(&self, ui: &mut Ui);

    /// Gains or loses the screen's keyboard focus.
    fn set_focused(&mut self, _focused: bool) {}

    fn contains(&self, x: f32, y: f32) -> bool {
        self.rect().contains(x, y)
    }
}
