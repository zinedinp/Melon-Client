//! Microsoft device-code login on a background thread the GUI can poll.

pub mod accounts;
mod http;
mod md5;
pub mod minecraft;
pub mod oauth;
mod secrets;
pub mod skin;
pub mod xbox;

use accounts::{Account, Accounts};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

pub enum LoginEvent {
    DeviceCode(oauth::DeviceCodeResponse),
    Success {
        username: String,
        #[allow(dead_code)]
        uuid: String,
    },
    Error(String),
    /// A saved session could not be refreshed; the button returns to logged out.
    SessionExpired,
}

pub struct LoginHandle {
    pub events: Receiver<LoginEvent>,
}

pub fn start_login(client_id: String, game_dir: PathBuf) -> LoginHandle {
    let (tx, rx) = channel();
    std::thread::spawn(move || run_login(&client_id, &game_dir, &tx));
    LoginHandle { events: rx }
}

/// Refreshes the selected Microsoft account's session, or checks that its token still works.
pub fn start_restore(client_id: Option<String>, game_dir: PathBuf) -> LoginHandle {
    let (tx, rx) = channel();
    std::thread::spawn(
        move || match restore_session(client_id.as_deref(), &game_dir) {
            Ok((username, uuid)) => {
                tx.send(LoginEvent::Success { username, uuid }).ok();
            }
            Err(_) => {
                tx.send(LoginEvent::SessionExpired).ok();
            }
        },
    );
    LoginHandle { events: rx }
}

fn run_login(client_id: &str, game_dir: &Path, tx: &Sender<LoginEvent>) {
    let result = (|| -> anyhow::Result<(String, String)> {
        let device = oauth::request_device_code(client_id)?;
        tx.send(LoginEvent::DeviceCode(device.clone())).ok();
        let ms_token = oauth::poll_for_token(client_id, &device)?;
        let stored = finish_microsoft_login(ms_token, game_dir, true)?;
        Ok((stored.username, stored.uuid))
    })();

    match result {
        Ok((username, uuid)) => {
            tx.send(LoginEvent::Success { username, uuid }).ok();
        }
        Err(e) => {
            tx.send(LoginEvent::Error(format!("{e:#}"))).ok();
        }
    }
}

/// Xbox Live → XSTS → Minecraft profile, then write it to `accounts.json`:
fn finish_microsoft_login(
    ms_token: oauth::MicrosoftAccessToken,
    game_dir: &Path,
    add: bool,
) -> anyhow::Result<StoredAccount> {
    let xbl = xbox::authenticate_xbox_live(&ms_token.access_token)?;
    let xsts = xbox::authorize_xbox_live(&xbl.token)?;
    let session = minecraft::login_with_xbox(&xsts)?;
    let profile = minecraft::fetch_profile(&session)?;
    let stored = StoredAccount {
        username: profile.name,
        uuid: profile.id,
        access_token: session.access_token,
        refresh_token: ms_token.refresh_token,
        in_keyring: false,
    };
    let mut accounts = Accounts::load(game_dir);
    accounts.store(Account::Microsoft(stored.clone()), add);
    accounts.save(game_dir)?;
    Ok(stored)
}

fn restore_session(client_id: Option<&str>, game_dir: &Path) -> anyhow::Result<(String, String)> {
    let Some(Account::Microsoft(account)) = Accounts::load(game_dir).selected().cloned() else {
        anyhow::bail!("kein gewaehltes Microsoft-Konto");
    };
    if let (Some(client_id), Some(refresh)) = (client_id, account.refresh_token.as_deref()) {
        let ms = oauth::refresh_access_token(client_id, refresh)?;
        let stored = finish_microsoft_login(ms, game_dir, false)?;
        return Ok((stored.username, stored.uuid));
    }
    let session = minecraft::MinecraftSession {
        access_token: account.access_token,
    };
    let profile = minecraft::fetch_profile(&session)?;
    Ok((profile.name, profile.id))
}

/// Melon's Azure Application (client) ID.
pub const PUBLIC_CLIENT_ID: &str = "061962da-2e9c-4d36-af8d-0b00b8828580";

/// Azure public-client ID for Melon's own app registration.
pub fn microsoft_client_id(home: &Path) -> Option<String> {
    from_env("MELON_MS_CLIENT_ID")
        .or_else(|| {
            option_env!("MELON_MS_CLIENT_ID")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| read_client_id(&home.join("client_id.txt")))
        .or_else(|| read_client_id(&crate::home::platform_data().join("client_id.txt")))
        .or_else(|| read_client_id(&crate::home::default_assets_dir().join("client_id.txt")))
        .or_else(|| read_client_id(&home.join("assets").join("client_id.txt")))
        .or_else(|| Some(PUBLIC_CLIENT_ID.to_owned()))
}

fn from_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_client_id(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Shown when the player asks to sign in and this build has no Azure app.
pub const SIGN_IN_NOT_CONFIGURED: &str = "Microsoft sign-in is not configured. Set MELON_MS_CLIENT_ID to your own Azure app (or put that ID in client_id.txt in the Melon data folder). Do not use another launcher's client ID.";

#[derive(Serialize, Deserialize, Clone)]
pub struct StoredAccount {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    /// Microsoft refresh token (`offline_access`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// The tokens are in the system keyring, not in `accounts.json`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_keyring: bool,
}

/// The account Play starts as, refreshing a Microsoft one when its session ran out.
pub fn load_or_refresh(game_dir: &Path) -> Option<Account> {
    let selected = Accounts::load(game_dir).selected()?.clone();
    let Account::Microsoft(account) = &selected else {
        return Some(selected);
    };
    let client_id = microsoft_client_id(game_dir);
    let (Some(refresh), Some(client_id)) = (account.refresh_token.clone(), client_id) else {
        return Some(selected);
    };
    match oauth::refresh_access_token(&client_id, &refresh)
        .and_then(|ms| finish_microsoft_login(ms, game_dir, false))
    {
        Ok(fresh) => Some(Account::Microsoft(fresh)),
        Err(e) => {
            crate::warn!("Microsoft session not refreshed: {e:#}");
            Some(selected)
        }
    }
}

/// The single account earlier versions kept in `account.json`.
fn legacy_account(game_dir: &Path) -> Option<StoredAccount> {
    let path = game_dir.join("account.json");
    let old = game_dir.join("assets").join("account.json");
    crate::json_file::load(&path).or_else(|| crate::json_file::load(&old))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_account_json_loads_without_a_refresh_token() {
        let account: StoredAccount =
            serde_json::from_str(r#"{"username":"Steve","uuid":"abc","access_token":"tok"}"#)
                .unwrap();
        assert_eq!(account.username, "Steve");
        assert!(account.refresh_token.is_none());
    }

    #[test]
    fn client_id_file_in_the_data_folder_is_enough() {
        let prev = std::env::var_os("MELON_MS_CLIENT_ID");
        unsafe { std::env::remove_var("MELON_MS_CLIENT_ID") };
        let dir = std::env::temp_dir().join(format!(
            "melon-client-id-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("client_id.txt"), "  abc-own-app  \n").unwrap();
        let id = microsoft_client_id(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        if let Some(v) = prev {
            unsafe { std::env::set_var("MELON_MS_CLIENT_ID", v) }
        }
        assert_eq!(id.as_deref(), Some("abc-own-app"));
    }

    #[test]
    fn the_public_client_id_is_used_when_nothing_overrides_it() {
        let prev = std::env::var_os("MELON_MS_CLIENT_ID");
        unsafe { std::env::remove_var("MELON_MS_CLIENT_ID") };
        let dir = std::env::temp_dir().join(format!(
            "melon-public-id-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&dir);
        let id = microsoft_client_id(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        if let Some(v) = prev {
            unsafe { std::env::set_var("MELON_MS_CLIENT_ID", v) }
        }
        assert_eq!(id.as_deref(), Some(PUBLIC_CLIENT_ID));
    }
}
