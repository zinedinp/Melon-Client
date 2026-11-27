//! Vanilla `EditWorldScreen`.

use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::widgets::Ui;
use crate::widgets::button::Button;
use crate::widgets::dialog::DialogScreen;
use crate::widgets::edit_box::EditBox;
use crate::widgets::layout::{BUTTON_H, BUTTON_W};
use crate::widgets::set::{KeyResult, WidgetSet};
use melon_net::singleplayer::{self, WorldEntry};

/// `EditWorldScreen.DEFAULT_WIDTH`, `HALF_WIDTH` and the layout's spacing.
const WIDE: f32 = 200.0;
const HALF: f32 = 98.0;
const ROW_SPACING: f32 = 5.0;
const HALF_GAP: f32 = 4.0;
/// `new SpacerElement(200, 20)` above the label and below the last button.
const SPACER_H: f32 = 20.0;
const LABEL_H: f32 = 9.0;
/// The name label, `ChatFormatting.GRAY`.
const GRAY: [f32; 4] = [170.0 / 255.0, 170.0 / 255.0, 170.0 / 255.0, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Name,
    ResetIcon,
    OpenFolder,
    Backup,
    BackupFolder,
    Optimize,
    Save,
    Cancel,
}

pub struct EditWorldScreen {
    folder: String,
    widgets: WidgetSet<Id>,
}

impl EditWorldScreen {
    pub fn new(entry: &WorldEntry, ctx: &ScreenContext) -> Self {
        let wide = |key| Button::sized(key, WIDE, BUTTON_H);
        let mut name = EditBox::new("selectWorld.enterName", WIDE, BUTTON_H);
        name.set_value(&entry.name);
        let mut widgets = WidgetSet::new()
            .with_field(Id::Name, name)
            .with_button(Id::ResetIcon, wide("selectWorld.edit.resetIcon"))
            .with_button(Id::OpenFolder, wide("selectWorld.edit.openFolder"))
            .with_button(Id::Backup, wide("selectWorld.edit.backup"))
            .with_button(Id::BackupFolder, wide("selectWorld.edit.backupFolder"))
            .with_button(Id::Optimize, wide("selectWorld.edit.optimize"))
            .with_button(
                Id::Save,
                Button::sized("selectWorld.edit.save", HALF, BUTTON_H),
            )
            .with_button(Id::Cancel, Button::sized("gui.cancel", HALF, BUTTON_H));
        let has_icon = singleplayer::icon_path(ctx.game_dir, &entry.folder).is_file();
        widgets.set_enabled(Id::ResetIcon, has_icon);
        widgets.set_enabled(Id::Optimize, false);
        widgets.focus(Id::Name);
        let mut screen = Self {
            folder: entry.folder.clone(),
            widgets,
        };
        screen.update_save();
        screen
    }

    /// `nameEdit.setResponder`: Save needs a name that isn't blank.
    fn update_save(&mut self) {
        let blank = self.widgets.field(Id::Name).value().trim().is_empty();
        self.widgets.set_enabled(Id::Save, !blank);
    }

    /// `onRename`, then leave.
    fn save(&self, ctx: &ScreenContext) -> ScreenAction {
        let name = self.widgets.field(Id::Name).value();
        if name.trim().is_empty() {
            return ScreenAction::None;
        }
        if let Err(e) = singleplayer::rename_world(ctx.game_dir, &self.folder, &name) {
            melon_base::warn!("World not renamed ({}): {e:#}", self.folder);
        }
        ScreenAction::Back
    }

    /// `makeWorldBackup`.
    fn backup(&self, ctx: &ScreenContext) -> ScreenAction {
        let dialog = DialogScreen::new();
        let dialog = match singleplayer::backup_world(ctx.game_dir, &self.folder) {
            Ok((_, bytes)) => {
                let megabytes = bytes.div_ceil(1_048_576).to_string();
                dialog
                    .line(
                        ctx.lang
                            .format("selectWorld.edit.backupCreated", &[&self.folder]),
                        [1.0; 4],
                    )
                    .line(
                        ctx.lang
                            .format("selectWorld.edit.backupSize", &[&megabytes]),
                        GRAY,
                    )
            }
            Err(e) => dialog
                .line("selectWorld.edit.backupFailed", [1.0, 0.33, 0.33, 1.0])
                .line(e.to_string(), GRAY),
        };
        ScreenAction::Replace(Box::new(
            dialog.button("gui.done", BUTTON_W, |_| ScreenAction::Back),
        ))
    }

    fn press(&mut self, id: Id, ctx: &ScreenContext) -> ScreenAction {
        ctx.click();
        match id {
            Id::Save => self.save(ctx),
            Id::Cancel => ScreenAction::Back,
            Id::ResetIcon => {
                let _ = std::fs::remove_file(singleplayer::icon_path(ctx.game_dir, &self.folder));
                self.widgets.set_enabled(Id::ResetIcon, false);
                ScreenAction::None
            }
            Id::OpenFolder => {
                open_folder(&singleplayer::world_dir(ctx.game_dir, &self.folder));
                ScreenAction::None
            }
            Id::BackupFolder => {
                let dir = singleplayer::backups_dir(ctx.game_dir);
                let _ = std::fs::create_dir_all(&dir);
                open_folder(&dir);
                ScreenAction::None
            }
            Id::Backup => self.backup(ctx),
            Id::Name | Id::Optimize => ScreenAction::None,
        }
    }
}

fn open_folder(path: &std::path::Path) {
    if let Err(e) = open::that(path) {
        melon_base::warn!("Folder not opened ({}): {e}", path.display());
    }
}

impl Screen for EditWorldScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        let total = SPACER_H + LABEL_H + BUTTON_H * 7.0 + SPACER_H + ROW_SPACING * 9.0;
        let mut y =
            ((height - total) / 2.0).trunc() + SPACER_H + ROW_SPACING + LABEL_H + ROW_SPACING;
        let x = ((width - WIDE) / 2.0).trunc();
        self.widgets.column(
            &[
                Id::Name,
                Id::ResetIcon,
                Id::OpenFolder,
                Id::Backup,
                Id::BackupFolder,
                Id::Optimize,
            ],
            x,
            y,
            BUTTON_H + ROW_SPACING,
        );
        y += (BUTTON_H + ROW_SPACING) * 6.0 + SPACER_H + ROW_SPACING;
        self.widgets
            .centered_row(&[Id::Save, Id::Cancel], width, y, HALF_GAP);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.widgets.hover(x, y);
    }

    fn cursor(&self) -> CursorKind {
        self.widgets.cursor().unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.click(x, y) {
            Some(Id::Name) | None => ScreenAction::None,
            Some(id) => self.press(id, ctx),
        }
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => self.press(id, ctx),
            KeyResult::Edited(_) => {
                self.update_save();
                ScreenAction::None
            }
            KeyResult::Ignored
                if matches!(event, TextEvent::Enter)
                    && self.widgets.focused() == Some(Id::Name) =>
            {
                self.save(ctx)
            }
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, _ctx: &ScreenContext) {
        ui.menu_background();
        let title = ui.tr("selectWorld.edit.title");
        ui.title(title, 15.0);
        let label = ui.tr("selectWorld.enterName");
        let field = self.widgets.field(Id::Name);
        ui.text(label, field.x, field.y - ROW_SPACING - LABEL_H, GRAY);
        self.widgets.draw(ui);
    }
}
