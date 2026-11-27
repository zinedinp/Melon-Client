//! Vanilla `ManageServerScreen` ("Edit Server Info").

use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::widgets::Ui;
use crate::widgets::button::Button;
use crate::widgets::edit_box::EditBox;
use crate::widgets::layout::{BUTTON_H, WIDE_BUTTON_W, quarter, wide_column_left};
use crate::widgets::set::{KeyResult, WidgetSet};
use melon_assets::assets::packs::server::Policy;
use melon_net::login::resolver::parse_address;
use melon_settings::server_list::{self, ServerEntry};

/// `0xA0A0A0`, the labels above the fields.
const LABEL_COLOR: [f32; 4] = [0.63, 0.63, 0.63, 1.0];
/// `EditBox`'s default `maxLength`, which the name keeps.
const NAME_MAX: usize = 32;
/// `ipEdit.setMaxLength(128)`.
const ADDRESS_MAX: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Name,
    Address,
    ResourcePack,
    Done,
    Cancel,
}

pub struct EditServerScreen {
    /// `Some(i)` edits saved server `i`; `None` adds a new one.
    index: Option<usize>,
    /// `manageServer.add.title` or `manageServer.edit.title`.
    title: &'static str,
    widgets: WidgetSet<Id>,
    /// The "Server Resource Packs" cycle button's value.
    pack_policy: Policy,
}

impl EditServerScreen {
    /// Add Server: `new ServerData("", "", ...)`, the default name only as the hint.
    pub fn new_add() -> Self {
        let entry = ServerEntry {
            name: String::new(),
            ip: String::new(),
            accept_textures: None,
        };
        Self::build(None, "manageServer.add.title", &entry)
    }

    pub fn new_edit(index: usize, entry: &ServerEntry) -> Self {
        Self::build(Some(index), "manageServer.edit.title", entry)
    }

    fn build(index: Option<usize>, title: &'static str, entry: &ServerEntry) -> Self {
        let mut name = EditBox::new("selectServer.defaultName", WIDE_BUTTON_W, BUTTON_H);
        name.set_max_length(NAME_MAX);
        name.set_value(&entry.name);
        let mut address = EditBox::new("", WIDE_BUTTON_W, BUTTON_H);
        address.set_max_length(ADDRESS_MAX);
        address.set_value(&entry.ip);
        let mut widgets = WidgetSet::new()
            .with_field(Id::Name, name)
            .with_field(Id::Address, address)
            .with_button(
                Id::ResourcePack,
                Button::sized("manageServer.resourcePack", WIDE_BUTTON_W, BUTTON_H),
            )
            .with_button(Id::Done, Button::sized("gui.done", WIDE_BUTTON_W, BUTTON_H))
            .with_button(
                Id::Cancel,
                Button::sized("gui.cancel", WIDE_BUTTON_W, BUTTON_H),
            );
        widgets.focus(Id::Name);
        let mut screen = Self {
            index,
            title,
            widgets,
            pack_policy: Policy::from_accept(entry.accept_textures),
        };
        screen.update_done_enabled();
        screen
    }

    fn value(&self, id: Id) -> String {
        self.widgets.field(id).value().trim().to_owned()
    }

    /// `updateAddButtonStatus`: `ServerAddress.isValidAddress`; the name may be blank.
    fn update_done_enabled(&mut self) {
        let ok = parse_address(&self.value(Id::Address)).is_some();
        self.widgets.set_enabled(Id::Done, ok);
    }

    /// Saves the server and goes back.
    fn save(&self, ctx: &ScreenContext) -> ScreenAction {
        if !self.widgets.button(Id::Done).enabled {
            return ScreenAction::None;
        }
        // `onAdd`: a blank name saves as the default one.
        let mut name = self.value(Id::Name);
        if name.is_empty() {
            ctx.lang.get("selectServer.defaultName").clone_into(&mut name);
        }
        server_list::upsert(
            ctx.game_dir,
            self.index,
            ServerEntry {
                name,
                ip: self.value(Id::Address),
                accept_textures: self.pack_policy.accept(),
            },
        );
        ScreenAction::Back
    }

    fn press(&mut self, id: Id, ctx: &ScreenContext) -> ScreenAction {
        match id {
            Id::Done => self.save(ctx),
            Id::Cancel => ScreenAction::Back,
            Id::ResourcePack => {
                self.pack_policy = self.pack_policy.next();
                ScreenAction::None
            }
            Id::Name | Id::Address => ScreenAction::None,
        }
    }
}

impl Screen for EditServerScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        let left = wide_column_left(width);
        self.widgets.set_position(Id::Name, left, 66.0);
        self.widgets.set_position(Id::Address, left, 106.0);
        self.widgets
            .set_position(Id::ResourcePack, left, quarter(height) + 72.0);
        self.widgets
            .column(&[Id::Done, Id::Cancel], left, quarter(height) + 114.0, 24.0);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.widgets.hover(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.widgets.cursor().unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.click(x, y) {
            Some(id @ (Id::Done | Id::Cancel | Id::ResourcePack)) => {
                ctx.click();
                self.press(id, ctx)
            }
            _ => ScreenAction::None,
        }
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => {
                ctx.click();
                self.press(id, ctx)
            }
            KeyResult::Edited(_) => {
                self.update_done_enabled();
                ScreenAction::None
            }
            KeyResult::Ignored if matches!(event, TextEvent::Enter) => self.save(ctx),
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let title = ui.tr(self.title);
        ui.title(title, 17.0);
        // `width / 2 - 100 + 1`.
        let left = wide_column_left(ui.width) + 1.0;
        let name_label = ui.tr("manageServer.enterName");
        ui.text(name_label, left, 53.0, LABEL_COLOR);
        let address_label = ui.tr("manageServer.enterIp");
        ui.text(address_label, left, 94.0, LABEL_COLOR);
        let caption = ui.tr("manageServer.resourcePack");
        let value_key = format!("manageServer.resourcePack.{}", self.pack_policy.key());
        let value = ui.lang.get_dynamic(&value_key);
        let label = ui.lang.format("options.generic_value", &[caption, value]);
        self.widgets
            .draw_with(ui, |id| (id == Id::ResourcePack).then(|| label.clone()));
    }
}
