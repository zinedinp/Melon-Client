//! Minecraft services login: xmcl `loginMinecraftWithXBox` + profile fetch.

use crate::auth::xbox::XboxLiveToken;
use serde::{Deserialize, Serialize};

pub struct MinecraftSession {
    pub access_token: String,
}

#[derive(Serialize)]
struct LoginRequest {
    #[serde(rename = "identityToken")]
    identity_token: String,
}

#[derive(Deserialize)]
struct LoginResponse {
    access_token: String,
}

/// `loginMinecraftWithXBox`.
pub fn login_with_xbox(xbox: &XboxLiveToken) -> anyhow::Result<MinecraftSession> {
    let body = LoginRequest {
        identity_token: format!("XBL3.0 x={};{}", xbox.user_hash, xbox.token),
    };
    let parsed: LoginResponse = super::http::json(
        "Minecraft-Login fehlgeschlagen",
        super::http::post("https://api.minecraftservices.com/authentication/login_with_xbox")
            .send_json(&body),
    )?;
    Ok(MinecraftSession {
        access_token: parsed.access_token,
    })
}

#[derive(Deserialize)]
pub struct MinecraftProfile {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
struct JoinServerRequest<'a> {
    #[serde(rename = "accessToken")]
    access_token: &'a str,
    #[serde(rename = "selectedProfile")]
    selected_profile: &'a str,
    #[serde(rename = "serverId")]
    server_id: &'a str,
}

/// `MinecraftSessionService.joinServer`.
pub fn join_server(access_token: &str, uuid: &str, server_id_hash: &str) -> anyhow::Result<()> {
    let body = JoinServerRequest {
        access_token,
        selected_profile: uuid,
        server_id: server_id_hash,
    };
    super::http::check(
        "Sitzungsserver hat den Beitritt abgelehnt",
        super::http::post("https://sessionserver.mojang.com/session/minecraft/join")
            .send_json(&body),
    )?;
    Ok(())
}

/// `GET /minecraft/profile`: ownership check, username, uuid.
pub fn fetch_profile(session: &MinecraftSession) -> anyhow::Result<MinecraftProfile> {
    super::http::json(
        "Minecraft-Profil konnte nicht geladen werden (Account besitzt evtl. kein Minecraft)",
        super::http::get("https://api.minecraftservices.com/minecraft/profile")
            .header("Authorization", format!("Bearer {}", session.access_token))
            .call(),
    )
}
