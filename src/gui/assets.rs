//! Every GUI sprite plus the font, uploaded once at startup.

use crate::assets::Assets;
use crate::gui::sprite::GuiSprite;
use crate::gui::text::font::{DYNAMIC_PAGE, Font};
use crate::gui::widgets::button::{ButtonTextures, TabTextures};
use crate::gui::widgets::row_icon::{Arrow, RowIconSprites};
use crate::gui::widgets::slider::SliderTextures;
use crate::renderer::{Renderer, TextureId};

/// `ServerSelectionList`'s sprites: the status bars and the icon overlays.
pub struct ServerListSprites {
    /// `ping_1` (worst) to `ping_5` (best).
    pub ping: [GuiSprite; 5],
    /// The five `pinging_*` frames, played while waiting for an answer.
    pub pinging: [GuiSprite; 5],
    pub incompatible: GuiSprite,
    pub unreachable: GuiSprite,
    /// The arrows over a hovered server: join, move up, move down.
    pub row: RowIconSprites,
    /// `textures/misc/unknown_server.png`: the icon of a server without one.
    pub unknown_server: TextureId,
}

/// The two nine-slices every tooltip is drawn with.
#[derive(Clone, Copy)]
pub struct TooltipSprites {
    pub background: GuiSprite,
    pub frame: GuiSprite,
}

impl TooltipSprites {
    /// Both sprites, or `None` if either is missing.
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> Option<Self> {
        let background = load(
            renderer,
            assets,
            "minecraft:gui/sprites/tooltip/background.png",
        );
        let frame = load(renderer, assets, "minecraft:gui/sprites/tooltip/frame.png");
        Some(Self {
            background: background.ok()?,
            frame: frame.ok()?,
        })
    }
}

pub struct GuiAssets {
    pub button_textures: ButtonTextures,
    pub logo: TextureId,
    pub edition: TextureId,
    pub language_icon: GuiSprite,
    pub accessibility_icon: GuiSprite,
    pub friends_icon: GuiSprite,
    /// `PauseScreen`'s three other `SpriteIconButton` sprites.
    pub bug_icon: GuiSprite,
    pub feedback_icon: GuiSprite,
    pub player_reporting_icon: GuiSprite,
    /// `RealmsNotificationsScreen.TRIAL_AVAILABLE_SPRITE`.
    pub realms_trial_icon: GuiSprite,
    pub white: TextureId,
    pub text_field: GuiSprite,
    pub text_field_highlighted: GuiSprite,
    pub list_background: TextureId,
    pub header_separator: TextureId,
    pub footer_separator: TextureId,
    pub scroller: GuiSprite,
    pub scroller_background: GuiSprite,
    /// `TooltipRenderUtil`'s `tooltip/background` and `tooltip/frame`.
    pub tooltip: Option<TooltipSprites>,
    /// `MenuTabBar` tab sprites (Create World's header).
    pub tab_textures: TabTextures,
    /// `AbstractSliderButton`'s track and handle.
    pub slider_textures: SliderTextures,
    /// `Checkbox`'s `widget/checkbox[_selected][_highlighted]`, chosen as a
    /// tab's are: selected or not, focused or not.
    pub checkbox: TabTextures,
    /// `DialogScreen.WARNING_BUTTON_SPRITES`: `dialog/warning_button*`.
    pub dialog_warning: ButtonTextures,
    pub server_list: ServerListSprites,
    /// The arrows over a hovered world (`world_list/join`).
    pub world_row: RowIconSprites,
    /// The arrows of a pack list (`transferable_list/*`): select, move.
    pub pack_row: RowIconSprites,
    /// The selected side's arrows.
    pub pack_selected_row: RowIconSprites,
    pub font: Font,
    pub font_textures: Vec<TextureId>,
    /// Fill plates instead of vanilla nine-slices (the standalone launcher).
    pub launcher: bool,
    /// Where the textures of `object` sprites are looked up.
    assets_root: Option<std::path::PathBuf>,
}

impl GuiAssets {
    /// Before a frame is drawn.
    pub fn sync_font(&mut self, renderer: &mut Renderer) {
        self.sync_objects(renderer);
        let work = self.font.bake_pending();
        if work.is_empty() {
            return;
        }
        for _ in 0..work.new_pages {
            let blank = image::RgbaImage::new(DYNAMIC_PAGE, DYNAMIC_PAGE);
            match renderer.load_texture(&blank) {
                Ok(id) => self.font_textures.push(id),
                Err(e) => {
                    crate::warn!("font: Unifont page: {e:#}");
                    self.font_textures.push(self.white);
                }
            }
        }
        for write in work.writes {
            if let Some(&texture) = self.font_textures.get(write.page) {
                renderer.update_texture(texture, 0, write.x, write.y, write.image);
            }
        }
    }

    /// The atlas sprites `object` components in the last frame's text asked for.
    fn sync_objects(&mut self, renderer: &mut Renderer) {
        use crate::component::ObjectInfo;
        use crate::gui::text::objects::{ObjectQuads, sprite_image};
        for info in self.font.take_wanted_objects() {
            let ObjectInfo::Atlas(sprite) = &info else {
                continue;
            };
            let image = self
                .assets_root
                .as_deref()
                .and_then(|root| sprite_image(root, sprite))
                .unwrap_or_else(crate::assets::missing_texture);
            let quads = renderer.load_texture(&image).ok().map(ObjectQuads::whole);
            self.font.set_object(info, quads);
        }
    }

    /// Every GUI sprite plus the font, uploaded once.
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let mut font = Font::load(assets, "minecraft:default")?;
        font.white = renderer.white_texture();

        let logo = standalone(renderer, assets, "minecraft:gui/title/minecraft.png")?;
        let edition = standalone(renderer, assets, "minecraft:gui/title/edition.png")?;
        let load = |renderer: &mut Renderer, id: &str| GuiSprite::load(renderer, assets, id);

        let language_icon = load(renderer, "minecraft:gui/sprites/icon/language.png")?;
        let accessibility_icon = load(renderer, "minecraft:gui/sprites/icon/accessibility.png")?;
        let friends_icon = load(renderer, "minecraft:gui/sprites/friends/friends.png")?;
        let bug_icon = load(renderer, "minecraft:gui/sprites/pause_menu/bug.png")?;
        let feedback_icon = load(
            renderer,
            "minecraft:gui/sprites/pause_menu/social_interactions.png",
        )?;
        let player_reporting_icon = load(
            renderer,
            "minecraft:gui/sprites/pause_menu/player_reporting.png",
        )?;
        let realms_trial_icon = load(renderer, "minecraft:gui/sprites/icon/trial_available.png")?;
        let button_normal = load(renderer, "minecraft:gui/sprites/widget/button.png")?;
        let button_highlighted = load(
            renderer,
            "minecraft:gui/sprites/widget/button_highlighted.png",
        )?;
        let button_disabled = load(renderer, "minecraft:gui/sprites/widget/button_disabled.png")?;
        let text_field = load(renderer, "minecraft:gui/sprites/widget/text_field.png")?;
        let text_field_highlighted = load(
            renderer,
            "minecraft:gui/sprites/widget/text_field_highlighted.png",
        )?;
        let tiled =
            |renderer: &mut Renderer, id: &str| renderer.load_texture_tiled(&assets.image(id)?);
        let list_background = tiled(renderer, "minecraft:gui/menu_list_background.png")?;
        let header_separator = tiled(renderer, "minecraft:gui/header_separator.png")?;
        let footer_separator = tiled(renderer, "minecraft:gui/footer_separator.png")?;
        let scroller = load(renderer, "minecraft:gui/sprites/widget/scroller.png")?;
        let scroller_background = load(
            renderer,
            "minecraft:gui/sprites/widget/scroller_background.png",
        )?;
        let tooltip = TooltipSprites::load(renderer, assets);
        let tab_textures = load_tabs(renderer, assets)?;
        let slider_textures = load_sliders(renderer, assets)?;
        let checkbox = load_checkbox(renderer, assets)?;
        let dialog_warning = ButtonTextures {
            normal: load(renderer, "minecraft:gui/sprites/dialog/warning_button.png")?,
            highlighted: load(
                renderer,
                "minecraft:gui/sprites/dialog/warning_button_highlighted.png",
            )?,
            disabled: load(
                renderer,
                "minecraft:gui/sprites/dialog/warning_button_disabled.png",
            )?,
        };
        let server_list = load_server_list(renderer, assets)?;
        let world_row = load_row_icons(renderer, assets, "world_list", "join", false)?;
        let pack_row = load_row_icons(renderer, assets, "transferable_list", "select", true)?;
        let pack_selected_row = RowIconSprites {
            primary: load_arrow(renderer, assets, "transferable_list", "unselect")?,
            up: pack_row.up,
            down: pack_row.down,
        };
        let white = renderer.white_texture();
        let font_textures = font
            .pages
            .iter()
            .map(|page| renderer.load_texture(page))
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(Self {
            button_textures: ButtonTextures {
                normal: button_normal,
                highlighted: button_highlighted,
                disabled: button_disabled,
            },
            logo,
            edition,
            language_icon,
            accessibility_icon,
            friends_icon,
            bug_icon,
            feedback_icon,
            player_reporting_icon,
            realms_trial_icon,
            white,
            text_field,
            text_field_highlighted,
            list_background,
            header_separator,
            footer_separator,
            scroller,
            scroller_background,
            tooltip,
            tab_textures,
            slider_textures,
            checkbox,
            dialog_warning,
            server_list,
            world_row,
            pack_row,
            pack_selected_row,
            font,
            font_textures,
            launcher: false,
            assets_root: Some(assets.root().to_path_buf()),
        })
    }

    /// Chrome the launcher can draw with no `assets/minecraft`.
    pub fn launcher(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let white = renderer.white_texture();
        let solid = GuiSprite::from(white);
        let page = crate::gui::ascii8::atlas();
        let mut font = crate::gui::ascii8::font(page.clone());
        font.white = white;
        let font_textures = vec![renderer.load_texture(&page)?];
        let logo = match image::load_from_memory(include_bytes!("../../packaging/melon.png")) {
            Ok(img) => renderer.load_texture(&img.to_rgba8()).unwrap_or(white),
            Err(_) => white,
        };
        let arrows = || RowIconSprites {
            primary: Arrow {
                normal: solid,
                highlighted: solid,
            },
            up: None,
            down: None,
        };
        Ok(Self {
            button_textures: ButtonTextures {
                normal: solid,
                highlighted: solid,
                disabled: solid,
            },
            logo,
            edition: white,
            language_icon: solid,
            accessibility_icon: solid,
            friends_icon: solid,
            bug_icon: solid,
            feedback_icon: solid,
            player_reporting_icon: solid,
            realms_trial_icon: solid,
            white,
            text_field: solid,
            text_field_highlighted: solid,
            list_background: white,
            header_separator: white,
            footer_separator: white,
            scroller: solid,
            scroller_background: solid,
            tooltip: None,
            tab_textures: TabTextures {
                normal: solid,
                highlighted: solid,
                selected: solid,
                selected_highlighted: solid,
            },
            slider_textures: SliderTextures {
                track: solid,
                track_highlighted: solid,
                handle: solid,
                handle_highlighted: solid,
            },
            checkbox: TabTextures {
                normal: solid,
                highlighted: solid,
                selected: solid,
                selected_highlighted: solid,
            },
            dialog_warning: ButtonTextures {
                normal: solid,
                highlighted: solid,
                disabled: solid,
            },
            server_list: ServerListSprites {
                ping: [solid; 5],
                pinging: [solid; 5],
                incompatible: solid,
                unreachable: solid,
                row: arrows(),
                unknown_server: white,
            },
            world_row: arrows(),
            pack_row: arrows(),
            pack_selected_row: arrows(),
            font,
            font_textures,
            launcher: true,
            assets_root: None,
        })
    }
}

fn load(renderer: &mut Renderer, assets: &Assets, id: &str) -> anyhow::Result<GuiSprite> {
    GuiSprite::load(renderer, assets, id)
}

/// A texture of its own rather than a sprite (`gui/title/*`).
fn standalone(renderer: &mut Renderer, assets: &Assets, id: &str) -> anyhow::Result<TextureId> {
    let image = assets.image(id)?;
    let (namespace, path) = crate::assets::split_id(id);
    let blur = crate::assets::texture_with_meta(
        assets.root(),
        &format!("{namespace}:{}", path.trim_end_matches(".png")),
    )
    .and_then(|(_, meta)| meta)
    .and_then(|meta| meta["texture"]["blur"].as_bool())
    .unwrap_or(false);
    if blur {
        renderer.load_texture_linear(&image)
    } else {
        renderer.load_texture(&image)
    }
}

/// `MenuTabBar.MenuTabButton.SPRITES`, in `WidgetSprites` order:
fn load_tabs(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<TabTextures> {
    let mut tab = |name: &str| {
        load(
            renderer,
            assets,
            &format!("minecraft:gui/sprites/widget/{name}.png"),
        )
    };
    Ok(TabTextures {
        selected: tab("tab_selected")?,
        normal: tab("tab")?,
        selected_highlighted: tab("tab_selected_highlighted")?,
        highlighted: tab("tab_highlighted")?,
    })
}

/// `Checkbox`'s four sprites.
fn load_checkbox(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<TabTextures> {
    let mut sprite = |name: &str| {
        load(
            renderer,
            assets,
            &format!("minecraft:gui/sprites/widget/{name}.png"),
        )
    };
    Ok(TabTextures {
        normal: sprite("checkbox")?,
        highlighted: sprite("checkbox_highlighted")?,
        selected: sprite("checkbox_selected")?,
        selected_highlighted: sprite("checkbox_selected_highlighted")?,
    })
}

fn load_sliders(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<SliderTextures> {
    let mut slider = |name: &str| {
        load(
            renderer,
            assets,
            &format!("minecraft:gui/sprites/widget/{name}.png"),
        )
    };
    Ok(SliderTextures {
        track: slider("slider")?,
        track_highlighted: slider("slider_highlighted")?,
        handle: slider("slider_handle")?,
        handle_highlighted: slider("slider_handle_highlighted")?,
    })
}

/// A normal and a highlighted arrow, `<folder>/<name>[_highlighted]`.
fn load_arrow(
    renderer: &mut Renderer,
    assets: &Assets,
    folder: &str,
    name: &str,
) -> anyhow::Result<Arrow> {
    let path = |suffix: &str| format!("minecraft:gui/sprites/{folder}/{name}{suffix}.png");
    Ok(Arrow {
        normal: load(renderer, assets, &path(""))?,
        highlighted: load(renderer, assets, &path("_highlighted"))?,
    })
}

/// The arrows of a list row.
fn load_row_icons(
    renderer: &mut Renderer,
    assets: &Assets,
    folder: &str,
    primary: &str,
    movable: bool,
) -> anyhow::Result<RowIconSprites> {
    let mut arrow = |name: &str| load_arrow(renderer, assets, folder, name);
    Ok(RowIconSprites {
        primary: arrow(primary)?,
        up: if movable {
            Some(arrow("move_up")?)
        } else {
            None
        },
        down: if movable {
            Some(arrow("move_down")?)
        } else {
            None
        },
    })
}

fn load_server_list(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<ServerListSprites> {
    let sprite = |renderer: &mut Renderer, name: &str| {
        load(
            renderer,
            assets,
            &format!("minecraft:gui/sprites/server_list/{name}.png"),
        )
    };
    let frames = |renderer: &mut Renderer, prefix: &str| -> anyhow::Result<[GuiSprite; 5]> {
        let mut frame = |n: u32| sprite(renderer, &format!("{prefix}_{n}"));
        Ok([frame(1)?, frame(2)?, frame(3)?, frame(4)?, frame(5)?])
    };
    Ok(ServerListSprites {
        ping: frames(renderer, "ping")?,
        pinging: frames(renderer, "pinging")?,
        incompatible: sprite(renderer, "incompatible")?,
        unreachable: sprite(renderer, "unreachable")?,
        row: load_row_icons(renderer, assets, "server_list", "join", true)?,
        unknown_server: assets.texture(renderer, "minecraft:misc/unknown_server.png")?,
    })
}
