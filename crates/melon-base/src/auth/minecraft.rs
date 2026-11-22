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
        "Minecraft login failed",
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

/// Mojang's session server, which Microsoft accounts join through.
pub const MOJANG_SESSION: &str = "https://sessionserver.mojang.com";

/// `MinecraftSessionService.joinServer`, on the session server under `session_base`: Mojang's,
/// or another sign-in service's.
pub fn join_server(
    session_base: &str,
    access_token: &str,
    uuid: &str,
    server_id_hash: &str,
) -> anyhow::Result<()> {
    let body = JoinServerRequest {
        access_token,
        selected_profile: uuid,
        server_id: server_id_hash,
    };
    super::http::check(
        "The session server refused the join",
        super::http::post(&format!("{session_base}/session/minecraft/join")).send_json(&body),
    )?;
    Ok(())
}

/// `GET /minecraft/profile`: ownership check, username, uuid.
pub fn fetch_profile(session: &MinecraftSession) -> anyhow::Result<MinecraftProfile> {
    super::http::json(
        "The Minecraft profile could not be loaded (the account may not own Minecraft)",
        super::http::get("https://api.minecraftservices.com/minecraft/profile")
            .header("Authorization", format!("Bearer {}", session.access_token))
            .call(),
    )
}
