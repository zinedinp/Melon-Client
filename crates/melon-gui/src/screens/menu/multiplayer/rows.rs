//! What the server list knows about each server.

use super::*;

/// One server's answer, ready to draw.
pub(super) struct Answer {
    pub(super) status: ServerStatus,
    /// `description`, as a component (`None` when it isn't one).
    pub(super) motd: Option<Component>,
    /// The MOTD's first two lines, wrapped, with where their sprites go;
    pub(super) motd_lines: OnceCell<Vec<MotdLine>>,
    /// `"<online>/<max>"` in `ServerStatusPinger.formatPlayerCount`'s colours.
    pub(super) players: StyledText,
    /// `protocol` matches this client.
    pub(super) compatible: bool,
}

/// One wrapped MOTD line and its sprite placeholders (`sprite_slots`).
pub(super) struct MotdLine {
    pub(super) text: StyledText,
    pub(super) sprites: Vec<(f32, usize)>,
}

pub(super) enum Ping {
    Pinging,
    Done(Box<Answer>),
    /// `UNREACHABLE`; `resolve` when it was the hostname that failed.
    Failed {
        resolve: bool,
    },
}

/// The server at the same index in `servers`: its answer and icon.
pub(super) struct Row {
    /// Tells a late answer which row it is for, whatever order the rows are in by then.
    pub(super) id: u64,
    pub(super) ping: Ping,
    pub(super) icon: Option<TextureId>,
    /// A favicon that came in and still has to go to the GPU.
    pub(super) pending_icon: Option<RgbaImage>,
    /// The MOTD's inline sprites, once loaded (`None` where a texture is missing).
    pub(super) sprites: Option<Vec<Option<TextureId>>>,
}

pub(super) type PingResult = (u64, u64, Result<ServerStatus, bool>);

/// MOTD sprites already on the GPU, by texture id; `None` when the file is missing.
pub(super) fn uploaded_sprites() -> &'static Mutex<HashMap<String, Option<TextureId>>> {
    static SPRITES: OnceLock<Mutex<HashMap<String, Option<TextureId>>>> = OnceLock::new();
    SPRITES.get_or_init(Default::default)
}

/// A sprite's texture.
pub(super) fn load_sprite(assets_root: &std::path::Path, id: &str) -> Option<RgbaImage> {
    let image = melon_assets::assets::texture_image(assets_root, id)?;
    let side = image.width();
    Some(if image.height() > side {
        image::imageops::crop_imm(&image, 0, 0, side, side).to_image()
    } else {
        image
    })
}

/// Where the sprite placeholders of `line` sit, as (x offset, sprite).
pub(super) fn sprite_slots(font: &Font, line: &StyledText) -> Vec<(f32, usize)> {
    let mut slots = Vec::new();
    let mut x = 0.0;
    for span in &line.spans {
        for c in span.text.chars() {
            if let Some(index) =
                sprite_index(c).filter(|_| font.glyph_in(span.style.font.as_deref(), c).is_none())
            {
                slots.push((x, index));
            }
            x += font.advance_in(span.style.font.as_deref(), c)
                + if span.style.bold { 1.0 } else { 0.0 };
        }
    }
    slots
}

/// The server's version as the list shows it; a server that names none is old.
pub(super) fn version_label<'a>(ui: &Ui<'a>, status: &'a ServerStatus) -> &'a str {
    if status.version_name.is_empty() {
        ui.tr("multiplayer.status.old")
    } else {
        &status.version_name
    }
}

/// Which `ping_*` sprite a latency gets (0 is `ping_1`, the worst).
pub(super) fn ping_level(ms: u128) -> usize {
    match ms {
        0..150 => 4,
        150..300 => 3,
        300..600 => 2,
        600..1000 => 1,
        _ => 0,
    }
}

/// `ServerStatusPinger.formatPlayerCount`: grey numbers, dark grey slash.
pub(super) fn player_count(online: i64, max: i64) -> StyledText {
    let mut text = StyledText::default();
    let number = TextStyle::colored(colors::GRAY);
    text.push(&online.to_string(), &number);
    text.push("/", &TextStyle::colored(colors::DARK_GRAY));
    text.push(&max.to_string(), &number);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_picks_the_ping_sprite() {
        let levels: Vec<_> = [0, 149, 150, 299, 300, 599, 600, 999, 1000]
            .map(ping_level)
            .to_vec();
        assert_eq!(levels, [4, 4, 3, 3, 2, 2, 1, 1, 0]);
    }

    #[test]
    fn player_count_is_grey_with_a_dark_slash() {
        let text = player_count(3, 20);
        assert_eq!(text.to_plain(), "3/20");
        assert_eq!(text.spans.len(), 3);
        assert_eq!(text.spans[1].style.color, Some(colors::DARK_GRAY));
    }
}
