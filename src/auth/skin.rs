//! A player's skin from Mojang's session server.

use super::http;
use anyhow::Context;
use image::RgbaImage;
use serde::Deserialize;

#[derive(Deserialize)]
struct Profile {
    properties: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct Textures {
    textures: Skins,
}

#[derive(Deserialize)]
struct Skins {
    #[serde(rename = "SKIN")]
    skin: Option<Skin>,
}

#[derive(Deserialize)]
struct Skin {
    url: String,
    metadata: Option<SkinMetadata>,
}

#[derive(Deserialize)]
struct SkinMetadata {
    model: Option<String>,
}

/// A downloaded skin, 64x64, and whether it is for slim arms.
pub struct DownloadedSkin {
    pub image: RgbaImage,
    pub slim: bool,
}

/// The head of the account `uuid` (32 hex digits), as it is on Mojang's skin.
pub fn fetch_head(uuid: &str) -> anyhow::Result<RgbaImage> {
    let skin = fetch_skin(uuid)?;
    head_from_skin(&skin.image).context("Skin hat kein Gesicht")
}

/// The skin of the account `uuid` (with or without dashes), made a 64x64 image.
pub fn fetch_skin(uuid: &str) -> anyhow::Result<DownloadedSkin> {
    let uuid = uuid.replace('-', "");
    let url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}");
    let profile: Profile = http::json("Profil", http::get(&url).call())?;
    let value = profile
        .properties
        .iter()
        .find(|p| p.name == "textures")
        .context("profile without a textures property")?
        .value
        .as_str();
    skin_from_textures(value)
}

/// The skin of the account called `name`.
pub fn fetch_skin_by_name(name: &str) -> anyhow::Result<DownloadedSkin> {
    #[derive(Deserialize)]
    struct Found {
        id: String,
    }
    anyhow::ensure!(
        !name.is_empty()
            && name.len() <= 16
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "kein gültiger Spielername: {name}"
    );
    let url = format!("https://api.mojang.com/users/profiles/minecraft/{name}");
    let found: Found = http::json("Profil", http::get(&url).call())?;
    fetch_skin(&found.id)
}

/// The skin a profile's `textures` property (Base64 JSON, as Mojang and servers send it) points
/// at.
pub fn skin_from_textures(value: &str) -> anyhow::Result<DownloadedSkin> {
    let decoded = crate::util::base64::decode(value).context("textures property is not base64")?;
    let textures: Textures = serde_json::from_slice(&decoded).with_context(|| {
        let start: String = String::from_utf8_lossy(&decoded).chars().take(60).collect();
        format!("textures property unreadable, starts with {start:?}")
    })?;
    let skin = textures.textures.skin.context("no custom skin")?;
    let slim = skin
        .metadata
        .and_then(|m| m.model)
        .is_some_and(|model| model == "slim");
    let url = skin.url.replacen("http://", "https://", 1);
    anyhow::ensure!(allowed_texture_url(&url), "Skin von fremdem Server: {url}");
    let mut response = http::check("Skin", http::get(&url).call())?;
    let bytes = response.body_mut().read_to_vec()?;
    let image = process_legacy_skin(image::load_from_memory(&bytes)?.to_rgba8())
        .context("Skin hat eine falsche Größe")?;
    Ok(DownloadedSkin { image, slim })
}

/// authlib's `TextureUrlChecker`.
fn allowed_texture_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.contains(['@', ':']) {
        return false;
    }
    ["minecraft.net", "mojang.com"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// `SkinTextureDownloader.processLegacySkin`.
pub fn process_legacy_skin(image: RgbaImage) -> Option<RgbaImage> {
    if image.width() != 64 || !(image.height() == 32 || image.height() == 64) {
        return None;
    }
    let legacy = image.height() == 32;
    let mut image = if legacy {
        let mut full = RgbaImage::new(64, 64);
        image::imageops::replace(&mut full, &image, 0, 0);
        for (x, y, dx, dy, w, h) in [
            (4, 16, 16, 32, 4, 4),
            (8, 16, 16, 32, 4, 4),
            (0, 20, 24, 32, 4, 12),
            (4, 20, 16, 32, 4, 12),
            (8, 20, 8, 32, 4, 12),
            (12, 20, 16, 32, 4, 12),
            (44, 16, -8, 32, 4, 4),
            (48, 16, -8, 32, 4, 4),
            (40, 20, 0, 32, 4, 12),
            (44, 20, -8, 32, 4, 12),
            (48, 20, -16, 32, 4, 12),
            (52, 20, -8, 32, 4, 12),
        ] {
            for row in 0..h {
                for column in 0..w {
                    let pixel = *full.get_pixel((x + column) as u32, (y + row) as u32);
                    let tx = x + dx + (w - 1 - column);
                    let ty = y + dy + row;
                    full.put_pixel(tx as u32, ty as u32, pixel);
                }
            }
        }
        full
    } else {
        image
    };
    set_no_alpha(&mut image, 0, 0, 32, 16);
    if legacy {
        notch_transparency_hack(&mut image, 32, 0, 64, 32);
    }
    set_no_alpha(&mut image, 0, 16, 64, 32);
    set_no_alpha(&mut image, 16, 48, 48, 64);
    Some(image)
}

fn set_no_alpha(image: &mut RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) {
    for y in y0..y1 {
        for x in x0..x1 {
            image.get_pixel_mut(x, y)[3] = 255;
        }
    }
}

/// `doNotchTransparencyHack`.
fn notch_transparency_hack(image: &mut RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) {
    for y in y0..y1 {
        for x in x0..x1 {
            if image.get_pixel(x, y)[3] < 128 {
                return;
            }
        }
    }
    for y in y0..y1 {
        for x in x0..x1 {
            image.get_pixel_mut(x, y)[3] = 0;
        }
    }
}

/// The face (8,8) of a 64-wide skin with the hat layer (40,8) over it.
pub fn head_from_skin(skin: &RgbaImage) -> Option<RgbaImage> {
    if skin.width() < 64 || skin.height() < 16 {
        return None;
    }
    let mut head = RgbaImage::new(8, 8);
    for y in 0..8 {
        for x in 0..8 {
            let face = *skin.get_pixel(8 + x, 8 + y);
            let hat = *skin.get_pixel(40 + x, 8 + y);
            head.put_pixel(x, y, if hat[3] > 0 { hat } else { face });
        }
    }
    Some(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn only_mojang_hosts_serve_skins() {
        assert!(allowed_texture_url(
            "https://textures.minecraft.net/texture/ab"
        ));
        assert!(allowed_texture_url("https://minecraft.net/x"));
        assert!(allowed_texture_url("https://a.mojang.com/x"));
        assert!(!allowed_texture_url(
            "http://textures.minecraft.net/texture/ab"
        ));
        assert!(!allowed_texture_url("https://evilminecraft.net/x"));
        assert!(!allowed_texture_url(
            "https://textures.minecraft.net.evil.com/x"
        ));
        assert!(!allowed_texture_url(
            "https://textures.minecraft.net@evil.com/x"
        ));
        assert!(!allowed_texture_url("https://localhost:8080/x"));
    }

    #[test]
    fn the_hat_covers_the_face_only_where_it_is_not_clear() {
        let mut skin = RgbaImage::new(64, 64);
        for y in 8..16 {
            for x in 8..16 {
                skin.put_pixel(x, y, Rgba([1, 2, 3, 255]));
            }
        }
        skin.put_pixel(40, 8, Rgba([9, 9, 9, 255]));
        let head = head_from_skin(&skin).unwrap();
        assert_eq!(head.get_pixel(0, 0), &Rgba([9, 9, 9, 255]));
        assert_eq!(head.get_pixel(1, 0), &Rgba([1, 2, 3, 255]));
        assert!(head_from_skin(&RgbaImage::new(8, 8)).is_none());
    }

    #[test]
    fn a_legacy_skin_gets_left_limbs_and_an_opaque_body() {
        let mut old = RgbaImage::new(64, 32);
        old.put_pixel(4, 20, Rgba([9, 9, 9, 255]));
        for y in 0..32 {
            for x in 32..64 {
                old.put_pixel(x, y, Rgba([1, 1, 1, 255]));
            }
        }
        let skin = process_legacy_skin(old).unwrap();
        assert_eq!(skin.height(), 64);
        assert_eq!(skin.get_pixel(23, 52)[0], 9);
        assert_eq!(skin.get_pixel(40, 8)[3], 0);
        assert_eq!(skin.get_pixel(20, 20)[3], 255);
    }

    #[test]
    fn odd_sizes_are_refused() {
        assert!(process_legacy_skin(RgbaImage::new(32, 32)).is_none());
        assert!(process_legacy_skin(RgbaImage::new(64, 64)).is_some());
    }
}
