//! Vanilla `SubtitleOverlay` ("Closed Captions"): the sounds just heard,
//! bottom right, each with an arrow toward where it came from when that is
//! out of view, fading from white to grey over three seconds.

use crate::hud::item_slot::ItemPainter;
use crate::text::{self, StyledText};
use melon_assets::i18n::Lang;
use melon_audio::Heard;
use melon_core::math::argb;
use melon_render::quad::DrawList;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// `SubtitleOverlay.DISPLAY_TIME`.
const DISPLAY_TIME: Duration = Duration::from_millis(3000);
/// `Font.lineHeight`.
const LINE_HEIGHT: f32 = 9.0;

/// Where the subtitles are heard from: the camera, as the sounds hear it.
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    pub position: [f64; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
}

/// `SubtitleOverlay.SoundPlayedAt`.
#[derive(Clone, Copy, Debug)]
struct PlayedAt {
    /// Where it was played; a sound without a place (`isRelative`: the UI,
    /// music) is at the world's origin, as vanilla's `getX/Y/Z` are 0.
    at: [f64; 3],
    time: Instant,
}

/// `SubtitleOverlay.Subtitle`.
struct Subtitle {
    key: Arc<str>,
    text: StyledText,
    range: f32,
    played_at: Vec<PlayedAt>,
}

fn distance_sq(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

impl Subtitle {
    /// `getBestSubtitleCandidate`: the newest place within range, the
    /// farther one on a tie; the only one if there is just one.
    fn best(&self, camera: [f64; 3]) -> Option<PlayedAt> {
        if self.played_at.len() == 1 {
            return self.played_at.first().copied();
        }
        let range = f64::from(self.range);
        self.played_at
            .iter()
            .filter(|p| distance_sq(p.at, camera) < range * range)
            .max_by(|a, b| {
                let far = |p: &PlayedAt| distance_sq(p.at, camera);
                a.time.cmp(&b.time).then(far(a).total_cmp(&far(b)))
            })
            .copied()
    }

    /// `isAudibleFrom`.
    fn audible_from(&self, camera: [f64; 3]) -> bool {
        if self.range.is_infinite() {
            return true;
        }
        let range = f64::from(self.range);
        self.best(camera)
            .is_some_and(|p| distance_sq(p.at, camera) < range * range)
    }

    /// `refresh`: the same place again replaces the old one.
    fn refresh(&mut self, at: [f64; 3], now: Instant) {
        self.played_at.retain(|p| p.at != at);
        self.played_at.push(PlayedAt { at, time: now });
    }

    /// `getBrightness` of a sound that does not loop: 255 when new, 75
    /// once its display time is over.
    fn brightness(played: PlayedAt, now: Instant, display_time: Duration) -> u8 {
        let age = now.saturating_duration_since(played.time).as_secs_f32();
        let t = (age / display_time.as_secs_f32()).clamp(0.0, 1.0);
        (255.0 + (75.0 - 255.0) * t).floor() as u8
    }
}

#[derive(Default)]
pub struct SubtitleOverlay {
    subtitles: Vec<Subtitle>,
}

impl SubtitleOverlay {
    /// `onPlaySound`: a subtitle already showing is refreshed instead.
    pub fn hear(&mut self, heard: Heard, lang: &Lang, now: Instant) {
        let at = heard.at.unwrap_or([0.0; 3]);
        if let Some(shown) = self.subtitles.iter_mut().find(|s| s.key == heard.subtitle) {
            shown.refresh(at, now);
            return;
        }
        self.subtitles.push(Subtitle {
            text: StyledText::plain(lang.get_dynamic(&heard.subtitle)),
            key: heard.subtitle,
            range: heard.range,
            played_at: vec![PlayedAt { at, time: now }],
        });
    }

    pub fn clear(&mut self) {
        self.subtitles.clear();
    }

    /// `extractRenderState`, onto a `size` GUI screen. `background` is
    /// `Options.getBackgroundColor(0.8F)`'s opacity.
    pub fn draw(
        &mut self,
        draw_list: &mut DrawList,
        painter: ItemPainter,
        listener: Listener,
        size: (f32, f32),
        background: f32,
        now: Instant,
    ) {
        let camera = listener.position;
        self.subtitles.retain_mut(|subtitle| {
            subtitle
                .played_at
                .retain(|p| now.saturating_duration_since(p.time) <= DISPLAY_TIME);
            !subtitle.played_at.is_empty()
        });
        let audible: Vec<&Subtitle> = self
            .subtitles
            .iter()
            .filter(|s| s.audible_from(camera))
            .collect();
        if audible.is_empty() {
            return;
        }
        let font = painter.font;
        let width_of = |s: &str| text::width(font, &StyledText::plain(s)).floor();
        let arrows = width_of("<") + width_of(" ") + width_of(">") + width_of(" ");
        let widest = audible
            .iter()
            .map(|s| text::width(font, &s.text).floor())
            .fold(0.0, f32::max);
        let width = widest + arrows;
        let half_width = (width / 2.0).floor();
        let half_height = (LINE_HEIGHT / 2.0).floor();
        let right = {
            let (f, u) = (listener.forward, listener.up);
            [
                f[1] * u[2] - f[2] * u[1],
                f[2] * u[0] - f[0] * u[2],
                f[0] * u[1] - f[1] * u[0],
            ]
        };
        let fill = argb(((background.clamp(0.0, 1.0) * 255.0) as u32) << 24);
        for (row, subtitle) in audible.iter().enumerate() {
            let Some(played) = subtitle.best(camera) else {
                continue;
            };
            let x = size.0 - half_width - 2.0;
            let y = size.1 - 35.0 - row as f32 * (LINE_HEIGHT + 1.0);
            draw_list.push_fill(
                painter.white,
                x - half_width - 1.0,
                y - half_height - 1.0,
                2.0 * half_width + 2.0,
                2.0 * half_height + 2.0,
                fill,
            );
            let grey = f32::from(Subtitle::brightness(played, now, DISPLAY_TIME)) / 255.0;
            let paint = painter.paint([grey; 3]);
            {
                let at = played.at;
                let delta = [0, 1, 2].map(|i| (at[i] - camera[i]) as f32);
                let length = delta.iter().map(|d| d * d).sum::<f32>().sqrt();
                // `Vec3.normalize`: a sound at the camera points nowhere.
                if length >= 1.0e-4 {
                    let along =
                        |axis: [f32; 3]| (0..3).map(|i| axis[i] * delta[i]).sum::<f32>() / length;
                    let in_view = along(listener.forward) > 0.5;
                    let rightness = along(right);
                    if !in_view && rightness > 0.0 {
                        let arrow = StyledText::plain(">");
                        let ax = x + half_width - text::width(font, &arrow);
                        text::draw(draw_list, &paint, &arrow, ax, y - half_height);
                    } else if !in_view && rightness < 0.0 {
                        let arrow = StyledText::plain("<");
                        text::draw(draw_list, &paint, &arrow, x - half_width, y - half_height);
                    }
                }
            }
            let text_width = text::width(font, &subtitle.text).floor();
            let tx = x - (text_width / 2.0).trunc();
            text::draw(draw_list, &paint, &subtitle.text, tx, y - half_height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(key: &str, at: Option<[f64; 3]>) -> Heard {
        Heard {
            subtitle: key.into(),
            at,
            range: if at.is_some() { 16.0 } else { f32::INFINITY },
        }
    }

    #[test]
    fn the_same_subtitle_is_refreshed_not_repeated() {
        let mut overlay = SubtitleOverlay::default();
        let lang = Lang::empty();
        let now = Instant::now();
        overlay.hear(
            heard("subtitles.block.generic.footsteps", Some([1.0; 3])),
            &lang,
            now,
        );
        overlay.hear(
            heard("subtitles.block.generic.footsteps", Some([2.0; 3])),
            &lang,
            now,
        );
        overlay.hear(
            heard("subtitles.block.generic.footsteps", Some([2.0; 3])),
            &lang,
            now,
        );
        assert_eq!(overlay.subtitles.len(), 1);
        assert_eq!(overlay.subtitles[0].played_at.len(), 2);
    }

    #[test]
    fn a_far_sound_is_not_audible_and_one_without_a_place_always_is() {
        let lang = Lang::empty();
        let now = Instant::now();
        let mut overlay = SubtitleOverlay::default();
        overlay.hear(heard("far", Some([100.0, 0.0, 0.0])), &lang, now);
        overlay.hear(heard("ui", None), &lang, now);
        let audible: Vec<&str> = overlay
            .subtitles
            .iter()
            .filter(|s| s.audible_from([0.0; 3]))
            .map(|s| &*s.key)
            .collect();
        assert_eq!(audible, ["ui"]);
        // Heard from the world's origin, so its arrow points there.
        let ui = overlay.subtitles.iter().find(|s| &*s.key == "ui").unwrap();
        assert_eq!(ui.best([50.0, 0.0, 0.0]).unwrap().at, [0.0; 3]);
    }

    #[test]
    fn a_subtitle_fades_from_white_to_grey() {
        let start = Instant::now();
        let played = PlayedAt {
            at: [0.0; 3],
            time: start,
        };
        assert_eq!(Subtitle::brightness(played, start, DISPLAY_TIME), 255);
        let later = start + DISPLAY_TIME;
        assert_eq!(Subtitle::brightness(played, later, DISPLAY_TIME), 75);
    }
}
