//! Xbox Live / XSTS, from xmcl `MicrosoftAuthenticator`

use anyhow::Context;
use serde::{Deserialize, Serialize};

pub struct XboxLiveToken {
    pub token: String,
    pub user_hash: String,
}

#[derive(Deserialize)]
struct XboxResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: String,
}

#[derive(Serialize)]
struct XblProperties<'a> {
    #[serde(rename = "AuthMethod")]
    auth_method: &'a str,
    #[serde(rename = "SiteName")]
    site_name: &'a str,
    #[serde(rename = "RpsTicket")]
    rps_ticket: String,
}

#[derive(Serialize)]
struct XblRequest<'a> {
    #[serde(rename = "Properties")]
    properties: XblProperties<'a>,
    #[serde(rename = "RelyingParty")]
    relying_party: &'a str,
    #[serde(rename = "TokenType")]
    token_type: &'a str,
}

/// `authenticateXboxLive`: Microsoft access token to Xbox Live user token.
pub fn authenticate_xbox_live(oauth_access_token: &str) -> anyhow::Result<XboxResponseRaw> {
    let body = XblRequest {
        properties: XblProperties {
            auth_method: "RPS",
            site_name: "user.auth.xboxlive.com",
            rps_ticket: format!("d={oauth_access_token}"),
        },
        relying_party: "http://auth.xboxlive.com",
        token_type: "JWT",
    };

    let parsed: XboxResponse = super::http::json(
        "Xbox-Live-Anmeldung fehlgeschlagen",
        super::http::post("https://user.auth.xboxlive.com/user/authenticate")
            .header("x-xbl-contract-version", "1")
            .send_json(&body),
    )?;
    Ok(XboxResponseRaw {
        token: parsed.token,
    })
}

pub struct XboxResponseRaw {
    pub token: String,
}

#[derive(Serialize)]
struct XstsProperties<'a> {
    #[serde(rename = "SandboxId")]
    sandbox_id: &'a str,
    #[serde(rename = "UserTokens")]
    user_tokens: [&'a str; 1],
}

#[derive(Serialize)]
struct XstsRequest<'a> {
    #[serde(rename = "Properties")]
    properties: XstsProperties<'a>,
    #[serde(rename = "RelyingParty")]
    relying_party: &'a str,
    #[serde(rename = "TokenType")]
    token_type: &'a str,
}

/// `authorizeXboxLive` for `rp://api.minecraftservices.com/`.
pub fn authorize_xbox_live(xbl_token: &str) -> anyhow::Result<XboxLiveToken> {
    let body = XstsRequest {
        properties: XstsProperties {
            sandbox_id: "RETAIL",
            user_tokens: [xbl_token],
        },
        relying_party: "rp://api.minecraftservices.com/",
        token_type: "JWT",
    };

    let parsed: XboxResponse = super::http::json(
        "XSTS-Autorisierung fehlgeschlagen (evtl. kein Xbox-Profil / regionale Sperre)",
        super::http::post("https://xsts.auth.xboxlive.com/xsts/authorize").send_json(&body),
    )?;
    let user_hash = parsed
        .display_claims
        .xui
        .into_iter()
        .next()
        .context("XSTS-Antwort ohne Benutzer-Hash")?
        .uhs;
    Ok(XboxLiveToken {
        token: parsed.token,
        user_hash,
    })
}
