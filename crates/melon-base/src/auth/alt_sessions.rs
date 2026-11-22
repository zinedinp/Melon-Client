//! Alternative sessions: sign-in services other than Microsoft (Ely.by, LittleSkin, Drasl,
//! Blessing Skin, ...). They speak the API of Mojang's old account service, codenamed
//! "Yggdrasil", as authlib-injector specifies it: one API root with `authserver/` and
//! `sessionserver/` under it, and the service's metadata at the root itself.

use super::http;
use anyhow::Context;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ureq::ResponseExt;

/// The header a website names its API root with (API Location Indication).
const ALI_HEADER: &str = "X-Authlib-Injector-API-Location";

/// A sign-in service the user picked.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Provider {
    /// The API root, without a trailing `/`.
    pub root: String,
    /// `meta.serverName`, else the host.
    pub name: String,
    /// Hosts textures may come from (`skinDomains`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skin_domains: Vec<String>,
    /// Where to make an account (`meta.links.register`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register_url: Option<String>,
    /// A profile name signs in as well as an email (`feature.non_email_login`).
    #[serde(default)]
    pub name_login: bool,
}

impl Provider {
    /// The root's host, which tells services apart.
    pub fn host(&self) -> &str {
        host_of(&self.root)
    }

    /// The base the `session/minecraft/...` paths go under.
    pub fn session_base(&self) -> String {
        format!("{}/sessionserver", self.root)
    }

    /// The Ely.by root: it wants a 2FA code after the password.
    pub fn is_ely(&self) -> bool {
        let host = self.host();
        host == "ely.by" || host.ends_with(".ely.by")
    }
}

/// A service offered without typing its address.
pub struct Preset {
    pub name: &'static str,
    pub address: &'static str,
    /// Signs in with the account's user name only, never its email (Drasl).
    pub name_only: bool,
}

pub const PRESETS: [Preset; 3] = [
    Preset {
        name: "Ely.by",
        address: "https://account.ely.by/api/authlib-injector",
        name_only: false,
    },
    Preset {
        name: "LittleSkin",
        address: "https://littleskin.cn/api/yggdrasil",
        name_only: false,
    },
    Preset {
        name: "Drasl",
        address: "https://drasl.unmojang.org/authlib-injector",
        name_only: true,
    },
];

fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(v6),
        None => authority.split(':').next().unwrap_or(authority),
    }
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// The address with `https://` in front where it has no scheme. Plain `http` only for this
/// computer: passwords never go out unencrypted.
fn with_scheme(input: &str) -> anyhow::Result<String> {
    let input = input.trim();
    anyhow::ensure!(!input.is_empty(), "No address given");
    if input.starts_with("https://") {
        return Ok(input.to_owned());
    }
    if input.starts_with("http://") {
        anyhow::ensure!(
            is_loopback(host_of(input)),
            "Only https addresses are allowed"
        );
        return Ok(input.to_owned());
    }
    anyhow::ensure!(!input.contains("://"), "Only https addresses are allowed");
    Ok(format!("https://{input}"))
}

/// `location` read against `base`, as a browser follows a link.
fn join_url(base: &str, location: &str) -> String {
    if location.contains("://") {
        return location.to_owned();
    }
    let (scheme, rest) = base.split_once("://").unwrap_or(("https", base));
    if let Some(rest) = location.strip_prefix("//") {
        return format!("{scheme}://{rest}");
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if location.starts_with('/') {
        return format!("{scheme}://{authority}{location}");
    }
    let path = rest.split(['?', '#']).next().unwrap_or(rest);
    let dir = path.rfind('/').map_or(path, |slash| &path[..=slash]);
    let dir = if dir.contains('/') {
        dir.to_owned()
    } else {
        format!("{dir}/")
    };
    format!("{scheme}://{dir}{location}")
}

#[derive(Deserialize, Default)]
struct Metadata {
    #[serde(default)]
    meta: Meta,
    #[serde(default, rename = "skinDomains")]
    skin_domains: Vec<String>,
}

#[derive(Deserialize, Default)]
struct Meta {
    #[serde(default, rename = "serverName")]
    server_name: Option<String>,
    #[serde(default)]
    links: Links,
    #[serde(default, rename = "feature.non_email_login")]
    non_email_login: bool,
}

#[derive(Deserialize, Default)]
struct Links {
    #[serde(default)]
    register: Option<String>,
}

fn parse_metadata(root: &str, body: &str) -> anyhow::Result<Provider> {
    let metadata: Metadata = serde_json::from_str(body).context(
        "This address is no sign-in service (its answer is not authlib-injector metadata)",
    )?;
    let name = metadata
        .meta
        .server_name
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| host_of(root).to_owned());
    Ok(Provider {
        root: root.to_owned(),
        name,
        skin_domains: metadata.skin_domains,
        register_url: metadata
            .meta
            .links
            .register
            .filter(|url| url.starts_with("https://")),
        name_login: metadata.meta.non_email_login,
    })
}

/// The service an address names: follows its API Location Indication to the API root and
/// reads the metadata there.
pub fn resolve(input: &str) -> anyhow::Result<Provider> {
    let url = with_scheme(input)?;
    let mut response = http::check("Sign-in service", http::get(&url).call())?;
    let landed = response.get_uri().to_string();
    let indicated = response
        .headers()
        .get(ALI_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(|location| join_url(&landed, location.trim()));
    let root = indicated.clone().unwrap_or_else(|| landed.clone());
    let root = root.trim_end_matches('/').to_owned();
    anyhow::ensure!(
        root.starts_with("https://") || is_loopback(host_of(&root)),
        "The service sent an address without https: {root}"
    );
    let body = if root == landed.trim_end_matches('/') {
        response.body_mut().read_to_string()?
    } else {
        fetch_metadata(&root)?
    };
    parse_metadata(&root, &body)
}

/// The metadata at the API root, as authlib-injector wants it prefetched.
pub fn fetch_metadata(root: &str) -> anyhow::Result<String> {
    let mut response = http::check("Sign-in service", http::get(root).call())?;
    Ok(response.body_mut().read_to_string()?)
}

/// A Minecraft profile of the account.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// 32 hex digits.
    pub id: String,
    pub name: String,
}

/// What signing in or refreshing gave.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub access_token: String,
    pub client_token: String,
    #[serde(default)]
    pub available_profiles: Vec<Profile>,
    #[serde(default)]
    pub selected_profile: Option<Profile>,
}

/// Ely.by refused the password alone: the account wants its 2FA code, sent as `password:code`.
#[derive(Debug)]
pub struct NeedsCode;

impl std::fmt::Display for NeedsCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("This account is protected with two-factor authentication")
    }
}

impl std::error::Error for NeedsCode {}

/// The service could not be asked: no network, or it is down. Not a verdict on the session.
#[derive(Debug)]
pub struct Unreachable(String);

impl std::fmt::Display for Unreachable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "The sign-in service cannot be reached: {}", self.0)
    }
}

impl std::error::Error for Unreachable {}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default, rename = "errorMessage")]
    error_message: String,
}

/// The service's own words for a refused request.
fn refusal(status: u16, body: &str) -> anyhow::Error {
    let Ok(parsed) = serde_json::from_str::<ErrorBody>(body) else {
        return anyhow::anyhow!("The sign-in service answered with HTTP {status}");
    };
    if parsed
        .error_message
        .to_ascii_lowercase()
        .contains("two factor")
    {
        return NeedsCode.into();
    }
    match (parsed.error_message.trim(), parsed.error.trim()) {
        ("", "") => anyhow::anyhow!("The sign-in service answered with HTTP {status}"),
        ("", error) => anyhow::anyhow!("{error}"),
        (message, _) => anyhow::anyhow!("{message}"),
    }
}

/// Posts `body` to `url`. `Ok(None)` for an empty success (204).
fn post<T: DeserializeOwned>(url: &str, body: &impl Serialize) -> anyhow::Result<Option<T>> {
    let mut response = http::post(url)
        .send_json(body)
        .map_err(|e| Unreachable(e.to_string()))?;
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().unwrap_or_default();
    if status >= 500 {
        return Err(Unreachable(format!("HTTP {status}")).into());
    }
    if !(200..300).contains(&status) {
        return Err(refusal(status, &text));
    }
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&text)
        .map(Some)
        .context("The sign-in service sent an unreadable answer")
}

#[derive(Serialize)]
struct Agent {
    name: &'static str,
    version: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Authenticate<'a> {
    agent: Agent,
    username: &'a str,
    password: &'a str,
    client_token: &'a str,
    request_user: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Refresh<'a> {
    access_token: &'a str,
    client_token: &'a str,
    request_user: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_profile: Option<&'a Profile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Tokens<'a> {
    access_token: &'a str,
    client_token: &'a str,
}

/// A fresh client token: one per sign-in.
pub fn new_client_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Signs in with an email (or a name, where the service allows it) and a password. Ely.by's
/// 2FA code goes after the password as `password:code`.
pub fn authenticate(
    provider: &Provider,
    login: &str,
    password: &str,
    client_token: &str,
) -> anyhow::Result<Session> {
    let body = Authenticate {
        agent: Agent {
            name: "Minecraft",
            version: 1,
        },
        username: login.trim(),
        password,
        client_token,
        request_user: false,
    };
    let url = format!("{}/authserver/authenticate", provider.root);
    post(&url, &body)?.context("The sign-in service sent no session")
}

/// A new access token for the session; with `profile`, binds it to that profile.
pub fn refresh(
    provider: &Provider,
    access_token: &str,
    client_token: &str,
    profile: Option<&Profile>,
) -> anyhow::Result<Session> {
    let body = Refresh {
        access_token,
        client_token,
        request_user: false,
        selected_profile: profile,
    };
    let url = format!("{}/authserver/refresh", provider.root);
    post(&url, &body)?.context("The sign-in service sent no session")
}

/// Whether the access token still works. An error only when the service cannot be asked.
pub fn validate(
    provider: &Provider,
    access_token: &str,
    client_token: &str,
) -> anyhow::Result<bool> {
    let body = Tokens {
        access_token,
        client_token,
    };
    let url = format!("{}/authserver/validate", provider.root);
    match post::<serde_json::Value>(&url, &body) {
        Ok(_) => Ok(true),
        Err(e) if e.is::<Unreachable>() => Err(e),
        Err(_) => Ok(false),
    }
}

/// Ends the session on the service; failures do not matter.
pub fn invalidate(provider: &Provider, access_token: &str, client_token: &str) {
    let body = Tokens {
        access_token,
        client_token,
    };
    let url = format!("{}/authserver/invalidate", provider.root);
    let _ = post::<serde_json::Value>(&url, &body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_get_https_and_plain_http_stays_local() {
        assert_eq!(
            with_scheme(" littleskin.cn ").unwrap(),
            "https://littleskin.cn"
        );
        assert_eq!(with_scheme("https://ely.by").unwrap(), "https://ely.by");
        assert_eq!(
            with_scheme("http://localhost:25585").unwrap(),
            "http://localhost:25585"
        );
        assert_eq!(
            with_scheme("http://127.0.0.1/x").unwrap(),
            "http://127.0.0.1/x"
        );
        assert!(with_scheme("http://ely.by").is_err());
        assert!(with_scheme("ftp://ely.by").is_err());
        assert!(with_scheme("").is_err());
    }

    #[test]
    fn the_api_location_is_read_against_the_page() {
        let page = "https://skin.example.com/user/home";
        assert_eq!(
            join_url(page, "/api/yggdrasil/"),
            "https://skin.example.com/api/yggdrasil/"
        );
        assert_eq!(
            join_url(page, "api/yggdrasil"),
            "https://skin.example.com/user/api/yggdrasil"
        );
        assert_eq!(
            join_url("https://skin.example.com", "api"),
            "https://skin.example.com/api"
        );
        assert_eq!(
            join_url(page, "https://other.example/api"),
            "https://other.example/api"
        );
        assert_eq!(
            join_url(page, "//cdn.example/api"),
            "https://cdn.example/api"
        );
    }

    #[test]
    fn hosts_are_read_without_port_or_user() {
        assert_eq!(
            host_of("https://littleskin.cn/api/yggdrasil"),
            "littleskin.cn"
        );
        assert_eq!(
            host_of("http://localhost:25585/authlib-injector"),
            "localhost"
        );
        assert_eq!(host_of("https://a@b.example:8443/x"), "b.example");
        assert_eq!(host_of("http://[::1]:80/"), "::1");
    }

    #[test]
    fn metadata_gives_name_domains_and_register_link() {
        let body = r#"{
            "meta": {
                "serverName": "Ely.by",
                "links": {"homepage": "https://ely.by", "register": "https://account.ely.by/register"},
                "feature.non_email_login": true
            },
            "skinDomains": ["ely.by", ".ely.by"],
            "signaturePublickey": "-----BEGIN PUBLIC KEY-----"
        }"#;
        let provider =
            parse_metadata("https://authserver.ely.by/api/authlib-injector", body).unwrap();
        assert_eq!(provider.name, "Ely.by");
        assert_eq!(provider.skin_domains, ["ely.by", ".ely.by"]);
        assert_eq!(
            provider.register_url.as_deref(),
            Some("https://account.ely.by/register")
        );
        assert!(provider.name_login && provider.is_ely());
        assert_eq!(
            provider.session_base(),
            "https://authserver.ely.by/api/authlib-injector/sessionserver"
        );

        let bare = parse_metadata("http://localhost:25585/authlib-injector", "{}").unwrap();
        assert_eq!(bare.name, "localhost");
        assert!(!bare.name_login && bare.skin_domains.is_empty());
        assert!(parse_metadata("https://example.com", "<html>").is_err());
    }

    #[test]
    fn refusals_use_the_services_words_and_spot_2fa() {
        let two_factor = r#"{"error":"ForbiddenOperationException","errorMessage":"Account protected with two factor auth."}"#;
        assert!(refusal(401, two_factor).is::<NeedsCode>());
        let wrong = r#"{"error":"ForbiddenOperationException","errorMessage":"Invalid credentials. Invalid email or password."}"#;
        assert_eq!(
            refusal(401, wrong).to_string(),
            "Invalid credentials. Invalid email or password."
        );
        assert_eq!(refusal(403, r#"{"error":"Nope"}"#).to_string(), "Nope");
        assert!(refusal(502, "<html>").to_string().contains("502"));
    }

    #[test]
    fn a_session_reads_with_and_without_a_selected_profile() {
        let one: Session = serde_json::from_str(
            r#"{"accessToken":"a","clientToken":"c","availableProfiles":[{"id":"0123","name":"Steve"}],"selectedProfile":{"id":"0123","name":"Steve"}}"#,
        )
        .unwrap();
        assert_eq!(one.selected_profile.unwrap().name, "Steve");
        let many: Session = serde_json::from_str(
            r#"{"accessToken":"a","clientToken":"c","availableProfiles":[{"id":"1","name":"A"},{"id":"2","name":"B"}]}"#,
        )
        .unwrap();
        assert!(many.selected_profile.is_none());
        assert_eq!(many.available_profiles.len(), 2);
    }

    /// Needs the network: `cargo test -p melon-base -- --ignored public_services`.
    #[test]
    #[ignore]
    fn public_services_resolve_from_their_websites() {
        let ely = resolve("ely.by").unwrap();
        assert_eq!(
            (ely.name.as_str(), ely.root.as_str()),
            ("Ely.by", PRESETS[0].address)
        );
        assert!(ely.is_ely() && ely.skin_domains.contains(&".ely.by".to_owned()));
        let little = resolve("littleskin.cn").unwrap();
        assert_eq!(
            (little.name.as_str(), little.root.as_str()),
            ("LittleSkin", PRESETS[1].address)
        );
        assert!(little.name_login);
        let drasl = resolve("drasl.unmojang.org").unwrap();
        assert_eq!(
            (drasl.name.as_str(), drasl.root.as_str()),
            ("Drasl", PRESETS[2].address)
        );
        for preset in &PRESETS {
            assert_eq!(resolve(preset.address).unwrap().name, preset.name);
        }
    }
}
