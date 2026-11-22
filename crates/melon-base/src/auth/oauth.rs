//! Microsoft OAuth2 device-code flow (RFC 8628). Access token feeds [`crate::auth::xbox`].

use anyhow::{Context, bail};
use serde::Deserialize;
use std::time::{Duration, Instant};

const TENANT: &str = "consumers";
const SCOPE: &str = "XboxLive.signin offline_access";

#[derive(Deserialize, Clone)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub expires_in: u64,
    pub interval: u64,
    /// Microsoft instruction text (URL + code), shown as-is.
    pub message: String,
    /// Verification URL for launching the browser (`message` is not a link).
    pub verification_uri: String,
    /// User code for the copyable box (`message` is not reliably parseable).
    pub user_code: String,
}

pub struct MicrosoftAccessToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

pub fn request_device_code(client_id: &str) -> anyhow::Result<DeviceCodeResponse> {
    let url = format!("https://login.microsoftonline.com/{TENANT}/oauth2/v2.0/devicecode");
    super::http::json(
        "Geraetecode konnte nicht angefragt werden",
        super::http::post(&url).send_form([("client_id", client_id), ("scope", SCOPE)]),
    )
}

#[derive(Deserialize)]
struct TokenErrorResponse {
    error: String,
}

#[derive(Deserialize)]
struct TokenSuccessResponse {
    access_token: String,
    refresh_token: Option<String>,
}

/// Poll the token endpoint until the user enters the code or `expires_in` elapses.
pub fn poll_for_token(
    client_id: &str,
    device: &DeviceCodeResponse,
) -> anyhow::Result<MicrosoftAccessToken> {
    let url = format!("https://login.microsoftonline.com/{TENANT}/oauth2/v2.0/token");
    let deadline = Instant::now() + Duration::from_secs(device.expires_in);
    let mut interval = Duration::from_secs(device.interval.max(1));

    loop {
        if Instant::now() >= deadline {
            bail!("Geraetecode abgelaufen - bitte erneut einloggen");
        }
        std::thread::sleep(interval);

        let mut response = super::http::post(&url)
            .send_form([
                ("client_id", client_id),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &device.device_code),
            ])
            .context("Token-Abfrage fehlgeschlagen (Netzwerk)")?;

        if response.status().is_success() {
            let success: TokenSuccessResponse = response
                .body_mut()
                .read_json()
                .context("ungueltige Token-Antwort")?;
            return Ok(MicrosoftAccessToken {
                access_token: success.access_token,
                refresh_token: success.refresh_token,
            });
        }

        let error: TokenErrorResponse =
            response
                .body_mut()
                .read_json()
                .unwrap_or(TokenErrorResponse {
                    error: "unknown_error".to_string(),
                });
        match error.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += Duration::from_secs(5),
            other => bail!("Microsoft-Login fehlgeschlagen: {other}"),
        }
    }
}

/// `grant_type=refresh_token`.
pub fn refresh_access_token(
    client_id: &str,
    refresh_token: &str,
) -> anyhow::Result<MicrosoftAccessToken> {
    let url = format!("https://login.microsoftonline.com/{TENANT}/oauth2/v2.0/token");
    let success: TokenSuccessResponse = super::http::json(
        "Microsoft-Sitzung konnte nicht erneuert werden",
        super::http::post(&url).send_form([
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ]),
    )?;
    Ok(MicrosoftAccessToken {
        access_token: success.access_token,
        refresh_token: success
            .refresh_token
            .or_else(|| Some(refresh_token.to_string())),
    })
}
