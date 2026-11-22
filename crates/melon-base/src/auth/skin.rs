//! A player's skin from Mojang's session server, or from the sign-in service the player uses.

use super::alt_sessions::Provider;
use super::http;
use anyhow::Context;
use image::RgbaImage;
use serde::Deserialize;
use std::sync::RwLock;

/// Where skins come from: Mojang, or another sign-in service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkinSource {
    /// The base of `session/minecraft/profile/<uuid>`.
    session: String,
    /// Another service's name lookup (`POST` a list of names); `None` for Mojang's.
    names: Option<String>,
    /// Texture hosts besides Mojang's, as `skinDomains` rules.
    domains: Vec<String>,
}

impl SkinSource {
    pub fn mojang() -> Self {
        Self {
            session: super::minecraft::MOJANG_SESSION.to_owned(),
            names: None,
            domains: Vec::new(),
        }
    }

    pub fn of(provider: &Provider) -> Self {
        Self {
            session: provider.session_base(),
            names: Some(format!("{}/api/profiles/minecraft", provider.root)),
            domains: provider.skin_domains.clone(),
        }
    }
}

/// Where this process looks up skins; Mojang until [`set_source`].
static SOURCE: RwLock<Option<SkinSource>> = RwLock::new(None);

/// Makes `source` where skins without one of their own come from: the client sets the service
/// its player signed in with.
pub fn set_source(source: SkinSource) {
    if let Ok(mut current) = SOURCE.write() {
        *current = Some(source);
    }
}

fn source() -> SkinSource {
    SOURCE
        .read()
        .ok()
        .and_then(|source| source.clone())
        .unwrap_or_else(SkinSource::mojang)
}

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

/// The head of the account `uuid` (32 hex digits), as it is on its skin.
pub fn fetch_head(uuid: &str) -> anyhow::Result<RgbaImage> {
    fetch_head_from(&source(), uuid)
}

/// [`fetch_head`] from a given service.
pub fn fetch_head_from(source: &SkinSource, uuid: &str) -> anyhow::Result<RgbaImage> {
    let skin = fetch_skin_from(source, uuid)?;
    head_from_skin(&skin.image).context("the skin has no face")
}

/// A profile property as a server hands it on: `textures`, with the service's signature.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct SignedProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

/// The account `uuid`'s properties with their signatures
/// (`session/minecraft/profile/<uuid>?unsigned=false`), from the service
/// skins come from: what a server sends other players so they see its skin.
pub fn fetch_signed_properties(uuid: &str) -> anyhow::Result<Vec<SignedProperty>> {
    #[derive(Deserialize)]
    struct Signed {
        properties: Vec<SignedProperty>,
    }
    let uuid = uuid.replace('-', "");
    let url = format!(
        "{}/session/minecraft/profile/{uuid}?unsigned=false",
        source().session
    );
    let profile: Signed = http::json("Profile", http::get(&url).call())?;
    Ok(profile.properties)
}

/// The skin of the account `uuid` (with or without dashes), made a 64x64 image.
pub fn fetch_skin(uuid: &str) -> anyhow::Result<DownloadedSkin> {
    fetch_skin_from(&source(), uuid)
}

/// [`fetch_skin`] from a given service.
pub fn fetch_skin_from(source: &SkinSource, uuid: &str) -> anyhow::Result<DownloadedSkin> {
    let uuid = uuid.replace('-', "");
    let url = format!("{}/session/minecraft/profile/{uuid}", source.session);
    let profile: Profile = http::json("Profile", http::get(&url).call())?;
    let value = profile
        .properties
        .iter()
        .find(|p| p.name == "textures")
        .context("profile without a textures property")?
        .value
        .as_str();
    skin_from_textures_of(source, value)
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
        "not a valid player name: {name}"
    );
    let source = source();
    let found: Found = match &source.names {
        None => {
            let url = format!("https://api.mojang.com/users/profiles/minecraft/{name}");
            http::json("Profile", http::get(&url).call())?
        }
        Some(url) => {
            let found: Vec<Found> = http::json("Profile", http::post(url).send_json([name]))?;
            found.into_iter().next().context("no such player")?
        }
    };
    fetch_skin_from(&source, &found.id)
}

/// The skin a profile's `textures` property (Base64 JSON, as Mojang and servers send it) points
/// at.
pub fn skin_from_textures(value: &str) -> anyhow::Result<DownloadedSkin> {
    skin_from_textures_of(&source(), value)
}

fn download(url: &str) -> anyhow::Result<Vec<u8>> {
    let mut response = http::check("Skin", http::get(url).call())?;
    Ok(response.body_mut().read_to_vec()?)
}

/// [`skin_from_textures`], with the texture hosts `source` allows.
fn skin_from_textures_of(source: &SkinSource, value: &str) -> anyhow::Result<DownloadedSkin> {
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
    // Mojang's links go over https; a service's are loaded as it hands them out (skins are
    // public, and a local server may have no https at all).
    let url = if allowed_texture_url(&skin.url, &source.domains) {
        skin.url.clone()
    } else {
        skin.url.replacen("http://", "https://", 1)
    };
    anyhow::ensure!(
        allowed_texture_url(&url, &source.domains),
        "skin from a host that is not allowed: {}",
        skin.url
    );
    let bytes = download(&url)?;
    let image = process_legacy_skin(image::load_from_memory(&bytes)?.to_rgba8())
        .context("the skin has a wrong size")?;
    Ok(DownloadedSkin { image, slim })
}

/// authlib's `TextureUrlChecker`: Mojang's hosts over https, plus the hosts the sign-in
/// service names (`skinDomains`), as authlib-injector allows them.
fn allowed_texture_url(url: &str, domains: &[String]) -> bool {
    let (secure, rest) = match (url.strip_prefix("https://"), url.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        (None, None) => return false,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return false;
    }
    let mojang = ["minecraft.net", "mojang.com"]
        .iter()
        .any(|domain| authority == *domain || authority.ends_with(&format!(".{domain}")));
    if secure && mojang {
        return true;
    }
    let host = match authority.split_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        Some(_) => return false,
        None => authority,
    };
    domains.iter().any(|rule| domain_rule_matches(rule, host))
}

/// One `skinDomains` rule: `.example.com` for its subdomains, `*` anywhere as a wildcard,
/// anything else for that host alone.
fn domain_rule_matches(rule: &str, host: &str) -> bool {
    let rule = rule.trim().to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    if rule.is_empty() || host.is_empty() {
        return false;
    }
    match rule.strip_prefix('.') {
        Some(parent) => wildcard(&format!("*.{parent}"), &host),
        None => wildcard(&rule, &host),
    }
}

/// `pattern` with `*` standing for any run of characters.
fn wildcard(pattern: &str, text: &str) -> bool {
    let Some((first, rest)) = pattern.split_once('*') else {
        return pattern == text;
    };
    let Some(mut remaining) = text.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = rest.split('*').collect();
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            return remaining.ends_with(part);
        }
        match remaining.find(part) {
            Some(at) => remaining = &remaining[at + part.len()..],
            None => return false,
        }
    }
    true
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
        let none: &[String] = &[];
        assert!(allowed_texture_url(
            "https://textures.minecraft.net/texture/ab",
            none
        ));
        assert!(allowed_texture_url("https://minecraft.net/x", none));
        assert!(allowed_texture_url("https://a.mojang.com/x", none));
        assert!(!allowed_texture_url(
            "http://textures.minecraft.net/texture/ab",
            none
        ));
        assert!(!allowed_texture_url("https://evilminecraft.net/x", none));
        assert!(!allowed_texture_url(
            "https://textures.minecraft.net.evil.com/x",
            none
        ));
        assert!(!allowed_texture_url(
            "https://textures.minecraft.net@evil.com/x",
            none
        ));
        assert!(!allowed_texture_url("https://localhost:8080/x", none));
    }

    #[test]
    fn a_services_skin_domains_add_hosts() {
        let ely = ["ely.by".to_owned(), ".ely.by".to_owned()];
        assert!(allowed_texture_url(
            "http://ely.by/storage/skins/a.png",
            &ely
        ));
        assert!(allowed_texture_url("https://skins.ely.by/a.png", &ely));
        assert!(!allowed_texture_url("https://notely.by/a.png", &ely));
        assert!(!allowed_texture_url("https://ely.by.evil.com/a.png", &ely));
        assert!(!allowed_texture_url("https://ely.by@evil.com/a.png", &ely));
        let local = ["localhost".to_owned()];
        assert!(allowed_texture_url(
            "http://localhost:25585/web/texture/skin/x.png",
            &local
        ));
        assert!(!allowed_texture_url("http://localhost:evil/x.png", &local));
        assert!(allowed_texture_url(
            "https://anything.example/x",
            &["*".to_owned()]
        ));
        assert!(domain_rule_matches("*.littleskin.cn", "a.littleskin.cn"));
        assert!(!domain_rule_matches("*.littleskin.cn", "littleskin.cn"));
        assert!(!domain_rule_matches(".ely.by", "ely.by"));
        assert!(domain_rule_matches(
            "skin-*.example.com",
            "skin-eu.example.com"
        ));
    }

    #[test]
    fn a_service_has_its_own_session_and_name_lookup() {
        let provider = Provider {
            root: "https://littleskin.cn/api/yggdrasil".into(),
            name: "LittleSkin".into(),
            skin_domains: vec!["littleskin.cn".into()],
            register_url: None,
            name_login: true,
        };
        let source = SkinSource::of(&provider);
        assert_eq!(
            source.session,
            "https://littleskin.cn/api/yggdrasil/sessionserver"
        );
        assert_eq!(
            source.names.as_deref(),
            Some("https://littleskin.cn/api/yggdrasil/api/profiles/minecraft")
        );
        assert_eq!(
            SkinSource::mojang().session,
            "https://sessionserver.mojang.com"
        );
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
