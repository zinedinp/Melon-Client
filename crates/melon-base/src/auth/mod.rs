//! Signing in on a background thread the GUI can poll: Microsoft's device code, or another
//! service's email and password.

pub mod accounts;
pub mod alt_sessions;
mod http;
mod md5;
pub mod minecraft;
pub mod oauth;
mod secrets;
pub mod skin;
pub mod xbox;

use accounts::{Account, Accounts, AltAccount};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

pub enum LoginEvent {
    DeviceCode(oauth::DeviceCodeResponse),
    Success {
        username: String,
    },
    Error(String),
    /// A saved session could not be refreshed; the button returns to logged out.
    SessionExpired,
    /// The account has several profiles: the user picks one, then [`start_alt_select`].
    ChooseProfile(PendingLogin),
    /// Ely.by wants the account's 2FA code after the password.
    NeedsCode,
}

/// A sign-in with another service that still needs a profile picked.
#[derive(Clone, Debug)]
pub struct PendingLogin {
    pub provider: alt_sessions::Provider,
    pub login: String,
    pub session: alt_sessions::Session,
}

pub struct LoginHandle {
    pub events: Receiver<LoginEvent>,
}

pub fn start_login(client_id: String, game_dir: PathBuf) -> LoginHandle {
    let (tx, rx) = channel();
    std::thread::spawn(move || run_login(&client_id, &game_dir, &tx));
    LoginHandle { events: rx }
}

/// Refreshes the selected account's session, or checks that its token still works.
pub fn start_restore(client_id: Option<String>, game_dir: PathBuf) -> LoginHandle {
    let (tx, rx) = channel();
    std::thread::spawn(
        move || match restore_session(client_id.as_deref(), &game_dir) {
            Ok(username) => {
                tx.send(LoginEvent::Success { username }).ok();
            }
            Err(_) => {
                tx.send(LoginEvent::SessionExpired).ok();
            }
        },
    );
    LoginHandle { events: rx }
}

/// Signs in with another service's email (or name) and password; Ely.by's 2FA code goes after
/// the password as `password:code`.
pub fn start_alt_login(
    provider: alt_sessions::Provider,
    login: String,
    password: String,
    game_dir: PathBuf,
) -> LoginHandle {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let client_token = alt_sessions::new_client_token();
        let event = match alt_sessions::authenticate(&provider, &login, &password, &client_token) {
            Ok(session) => {
                let pending = PendingLogin {
                    provider,
                    login,
                    session,
                };
                match (&pending.session.selected_profile, pending.session.available_profiles.as_slice()) {
                    (Some(profile), _) | (None, [profile]) => {
                        let profile = profile.clone();
                        finish_alt_event(pending, profile, &game_dir)
                    }
                    (None, []) => LoginEvent::Error(
                        "This account has no Minecraft profile yet. Create one on the service's website first."
                            .into(),
                    ),
                    (None, _) => LoginEvent::ChooseProfile(pending),
                }
            }
            Err(e) if e.is::<alt_sessions::NeedsCode>() => LoginEvent::NeedsCode,
            Err(e) => LoginEvent::Error(format!("{e:#}")),
        };
        tx.send(event).ok();
    });
    LoginHandle { events: rx }
}

/// Finishes a sign-in that had several profiles with the one the user picked.
pub fn start_alt_select(
    pending: PendingLogin,
    profile: alt_sessions::Profile,
    game_dir: PathBuf,
) -> LoginHandle {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        tx.send(finish_alt_event(pending, profile, &game_dir)).ok();
    });
    LoginHandle { events: rx }
}

fn finish_alt_event(
    pending: PendingLogin,
    profile: alt_sessions::Profile,
    game_dir: &Path,
) -> LoginEvent {
    match finish_alt_login(pending, profile, game_dir) {
        Ok(account) => LoginEvent::Success {
            username: account.username,
        },
        Err(e) => LoginEvent::Error(format!("{e:#}")),
    }
}

/// Binds the session to `profile` where it is not yet, then writes the account.
fn finish_alt_login(
    pending: PendingLogin,
    profile: alt_sessions::Profile,
    game_dir: &Path,
) -> anyhow::Result<AltAccount> {
    let PendingLogin {
        provider,
        login,
        mut session,
    } = pending;
    if session.selected_profile.as_ref() != Some(&profile) {
        session = alt_sessions::refresh(
            &provider,
            &session.access_token,
            &session.client_token,
            Some(&profile),
        )?;
    }
    let profile = session.selected_profile.unwrap_or(profile);
    let account = AltAccount {
        provider,
        username: profile.name,
        uuid: profile.id,
        login,
        access_token: session.access_token,
        client_token: session.client_token,
        in_keyring: false,
    };
    let mut accounts = Accounts::load(game_dir);
    accounts.store(Account::Alternative(account.clone()), true);
    accounts.save(game_dir)?;
    Ok(account)
}

/// The account with a working token: kept when the service accepts it, else refreshed and
/// saved.
pub fn refresh_alt(account: &AltAccount, game_dir: &Path) -> anyhow::Result<AltAccount> {
    let (provider, access, client) = (
        &account.provider,
        &account.access_token,
        &account.client_token,
    );
    if !access.is_empty() && alt_sessions::validate(provider, access, client)? {
        return Ok(account.clone());
    }
    let session = alt_sessions::refresh(provider, access, client, None)?;
    let mut fresh = account.clone();
    fresh.access_token = session.access_token;
    fresh.client_token = session.client_token;
    if let Some(profile) = session.selected_profile {
        fresh.username = profile.name;
    }
    let mut accounts = Accounts::load(game_dir);
    accounts.store(Account::Alternative(fresh.clone()), false);
    accounts.save(game_dir)?;
    Ok(fresh)
}

fn run_login(client_id: &str, game_dir: &Path, tx: &Sender<LoginEvent>) {
    let result = (|| -> anyhow::Result<String> {
        let device = oauth::request_device_code(client_id)?;
        tx.send(LoginEvent::DeviceCode(device.clone())).ok();
        let ms_token = oauth::poll_for_token(client_id, &device)?;
        let stored = finish_microsoft_login(ms_token, game_dir, true)?;
        Ok(stored.username)
    })();

    match result {
        Ok(username) => {
            tx.send(LoginEvent::Success { username }).ok();
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

fn restore_session(client_id: Option<&str>, game_dir: &Path) -> anyhow::Result<String> {
    let account = match Accounts::load(game_dir).selected().cloned() {
        Some(Account::Microsoft(account)) => account,
        Some(Account::Alternative(account)) => {
            let fresh = refresh_alt(&account, game_dir)?;
            return Ok(fresh.username);
        }
        _ => anyhow::bail!("no signed-in account is selected"),
    };
    if let (Some(client_id), Some(refresh)) = (client_id, account.refresh_token.as_deref()) {
        let ms = oauth::refresh_access_token(client_id, refresh)?;
        let stored = finish_microsoft_login(ms, game_dir, false)?;
        return Ok(stored.username);
    }
    let session = minecraft::MinecraftSession {
        access_token: account.access_token,
    };
    let profile = minecraft::fetch_profile(&session)?;
    Ok(profile.name)
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

/// The account Play starts as, refreshing its session when it ran out.
pub fn load_or_refresh(game_dir: &Path) -> Option<Account> {
    let selected = Accounts::load(game_dir).selected()?.clone();
    if let Account::Alternative(account) = &selected {
        return match refresh_alt(account, game_dir) {
            Ok(fresh) => Some(Account::Alternative(fresh)),
            Err(e) => {
                crate::warn!("{} session not refreshed: {e:#}", account.provider.name);
                Some(selected)
            }
        };
    }
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
