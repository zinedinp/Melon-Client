//! The accounts the launcher offers: Microsoft logins and offline profiles

use super::{StoredAccount, secrets};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The longest name a Minecraft profile can have.
pub const MAX_NAME_LEN: usize = 16;

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Account {
    Microsoft(StoredAccount),
    Offline { username: String },
}

impl Account {
    pub fn username(&self) -> &str {
        match self {
            Account::Microsoft(account) => &account.username,
            Account::Offline { username } => username,
        }
    }

    pub fn is_microsoft(&self) -> bool {
        matches!(self, Account::Microsoft(_))
    }

    /// What tells accounts apart: the Mojang UUID, or the offline name
    pub fn key(&self) -> String {
        match self {
            Account::Microsoft(account) => account.uuid.clone(),
            Account::Offline { username } => format!("offline:{}", username.to_lowercase()),
        }
    }
}

/// What the launcher keeps about an account besides how to sign in.
#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Meta {
    /// Play is refused for this account until it is unlocked.
    #[serde(default)]
    pub locked: bool,
    /// When the game last started with it, seconds since the Unix epoch.
    #[serde(default)]
    pub last_played: u64,
    /// Seconds the game ran with it in all.
    #[serde(default)]
    pub play_secs: u64,
}

/// The UUID an offline profile gets: vanilla's own derivation for a client
pub fn offline_uuid(name: &str) -> uuid::Uuid {
    uuid::Builder::from_md5_bytes(super::md5::digest(
        format!("OfflinePlayer:{name}").as_bytes(),
    ))
    .into_uuid()
}

/// Counts a game that ran `secs` with the account `key` in `accounts.json`.
pub fn record_play(game_dir: &Path, key: &str, secs: u64) {
    let mut accounts = Accounts::load(game_dir);
    accounts.record_play(key, secs, now_secs());
    if let Err(e) = accounts.save(game_dir) {
        crate::warn!("accounts.json not saved: {e}");
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A name an offline profile may have: 1 to 16 letters, digits or `_`.
pub fn valid_offline_name(name: &str) -> bool {
    (1..=MAX_NAME_LEN).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Accounts {
    /// The [`Account::key`] of the account Play starts as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected: Option<String>,
    #[serde(default)]
    accounts: Vec<Account>,
    /// By [`Account::key`].
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    meta: HashMap<String, Meta>,
}

/// The tokens that go to the keyring together.
#[derive(Serialize, Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
}

impl Accounts {
    /// `accounts.json`, or the single account an earlier version kept in
    pub fn load(game_dir: &Path) -> Self {
        if let Some(mut accounts) = crate::json_file::load::<Accounts>(&path(game_dir)) {
            accounts.take_from_keyring();
            return accounts;
        }
        match super::legacy_account(game_dir) {
            Some(account) => {
                let account = Account::Microsoft(account);
                Self {
                    selected: Some(account.key()),
                    accounts: vec![account],
                    meta: HashMap::new(),
                }
            }
            None => Self::default(),
        }
    }

    /// Writes `accounts.json`, readable by its owner only. The tokens of
    pub fn save(&self, game_dir: &Path) -> std::io::Result<()> {
        let mut on_disk = self.clone();
        on_disk.put_in_keyring();
        let file = path(game_dir);
        crate::json_file::save_private(&file, &on_disk)
    }

    /// Moves the tokens of every account that has them to the keyring and
    fn put_in_keyring(&mut self) {
        for account in &mut self.accounts {
            let Account::Microsoft(account) = account else {
                continue;
            };
            if account.access_token.is_empty() {
                continue;
            }
            let tokens = Tokens {
                access_token: account.access_token.clone(),
                refresh_token: account.refresh_token.clone(),
            };
            let kept = serde_json::to_string(&tokens)
                .is_ok_and(|secret| secrets::store(&account.uuid, &secret));
            account.in_keyring = kept;
            if kept {
                account.access_token.clear();
                account.refresh_token = None;
            }
        }
    }

    /// Fills in the tokens of accounts that were saved to the keyring.
    fn take_from_keyring(&mut self) {
        for account in &mut self.accounts {
            let Account::Microsoft(account) = account else {
                continue;
            };
            if !account.in_keyring || !account.access_token.is_empty() {
                continue;
            }
            let tokens = secrets::load(&account.uuid)
                .and_then(|secret| serde_json::from_str::<Tokens>(&secret).ok());
            if let Some(tokens) = tokens {
                account.access_token = tokens.access_token;
                account.refresh_token = tokens.refresh_token;
            }
        }
    }

    pub fn list(&self) -> &[Account] {
        &self.accounts
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// The list index of the account Play starts as.
    pub fn selected_index(&self) -> Option<usize> {
        let key = self.selected.as_deref()?;
        self.accounts.iter().position(|a| a.key() == key)
    }

    pub fn selected(&self) -> Option<&Account> {
        self.selected_index().map(|i| &self.accounts[i])
    }

    pub fn select(&mut self, index: usize) {
        if let Some(account) = self.accounts.get(index) {
            self.selected = Some(account.key());
        }
    }

    /// Adds an offline profile and selects it (an existing one of that
    pub fn add_offline(&mut self, name: &str) -> bool {
        if !valid_offline_name(name) {
            return false;
        }
        let account = Account::Offline {
            username: name.to_owned(),
        };
        self.store(account, true);
        true
    }

    /// Saves a Microsoft account's fresh tokens. With `add` it joins the
    pub fn store(&mut self, account: Account, add: bool) {
        let key = account.key();
        match self.accounts.iter().position(|a| a.key() == key) {
            Some(index) => self.accounts[index] = account,
            None if add => self.accounts.push(account),
            None => return,
        }
        if add {
            self.selected = Some(key);
        }
    }

    /// Swaps the account at `index` with its neighbour `by` places away
    pub fn move_account(&mut self, index: usize, by: isize) -> bool {
        let Some(target) = index.checked_add_signed(by) else {
            return false;
        };
        if index >= self.accounts.len() || target >= self.accounts.len() {
            return false;
        }
        self.accounts.swap(index, target);
        true
    }

    /// Removes an account; the selection moves to the first one left.
    pub fn remove(&mut self, index: usize) {
        if index >= self.accounts.len() {
            return;
        }
        let removed = self.accounts.remove(index);
        self.meta.remove(&removed.key());
        if let Account::Microsoft(account) = &removed {
            secrets::delete(&account.uuid);
        }
        if self.selected.as_deref() == Some(removed.key().as_str()) {
            self.selected = self.accounts.first().map(Account::key);
        }
    }
}

impl Accounts {
    pub fn meta(&self, account: &Account) -> Meta {
        self.meta.get(&account.key()).copied().unwrap_or_default()
    }

    pub fn set_locked(&mut self, index: usize, locked: bool) {
        if let Some(account) = self.accounts.get(index) {
            self.meta.entry(account.key()).or_default().locked = locked;
        }
    }

    /// The game ran for `secs` with the account `key`, ending `now`.
    pub fn record_play(&mut self, key: &str, secs: u64, now: u64) {
        if self.accounts.iter().any(|a| a.key() == key) {
            let meta = self.meta.entry(key.to_owned()).or_default();
            meta.last_played = now;
            meta.play_secs += secs;
        }
    }

    /// Gives the offline profile at `index` another name. That changes its
    pub fn rename_offline(&mut self, index: usize, name: &str) -> bool {
        let Some(Account::Offline { .. }) = self.accounts.get(index) else {
            return false;
        };
        let renamed = Account::Offline {
            username: name.to_owned(),
        };
        let taken = self
            .accounts
            .iter()
            .enumerate()
            .any(|(i, a)| i != index && a.key() == renamed.key());
        if !valid_offline_name(name) || taken {
            return false;
        }
        let old_key = self.accounts[index].key();
        let new_key = renamed.key();
        self.accounts[index] = renamed;
        if self.selected.as_deref() == Some(old_key.as_str()) {
            self.selected = Some(new_key.clone());
        }
        if let Some(meta) = self.meta.remove(&old_key) {
            self.meta.insert(new_key, meta);
        }
        true
    }
}

fn path(game_dir: &Path) -> PathBuf {
    game_dir.join("accounts.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_moves_by_swapping_with_its_neighbour() {
        let mut accounts = Accounts::default();
        for name in ["Ann", "Ben", "Cy"] {
            accounts.add_offline(name);
        }
        accounts.select(1);
        assert!(accounts.move_account(1, -1));
        let names: Vec<_> = accounts.list().iter().map(Account::username).collect();
        assert_eq!(names, ["Ben", "Ann", "Cy"]);
        assert_eq!(accounts.selected_index(), Some(0));
        assert!(!accounts.move_account(0, -1));
        assert!(!accounts.move_account(2, 1));
    }

    fn microsoft(name: &str, uuid: &str) -> Account {
        Account::Microsoft(StoredAccount {
            username: name.to_owned(),
            uuid: uuid.to_owned(),
            access_token: "token".to_owned(),
            refresh_token: None,
            in_keyring: false,
        })
    }

    #[test]
    fn offline_names_are_checked() {
        assert!(valid_offline_name("Steve_01"));
        assert!(!valid_offline_name(""));
        assert!(!valid_offline_name("has space"));
        assert!(!valid_offline_name("seventeen_chars_ab"));
        let mut accounts = Accounts::default();
        assert!(!accounts.add_offline("bad name"));
        assert!(accounts.is_empty());
    }

    #[test]
    fn an_offline_name_is_added_once_and_selected() {
        let mut accounts = Accounts::default();
        accounts.add_offline("Steve");
        accounts.add_offline("Alex");
        accounts.add_offline("STEVE");
        assert_eq!(accounts.list().len(), 2);
        assert_eq!(accounts.selected().unwrap().username(), "STEVE");
    }

    #[test]
    fn a_login_adds_and_selects_a_refresh_only_replaces() {
        let mut accounts = Accounts::default();
        accounts.store(microsoft("Old", "abc"), true);
        accounts.add_offline("Steve");
        accounts.store(microsoft("New", "abc"), false);
        assert_eq!(accounts.list()[0].username(), "New");
        assert_eq!(accounts.selected().unwrap().username(), "Steve");
        accounts.store(microsoft("Gone", "def"), false);
        assert_eq!(accounts.list().len(), 2);
    }

    #[test]
    fn removing_the_selection_selects_the_first_left() {
        let mut accounts = Accounts::default();
        accounts.store(microsoft("Main", "abc"), true);
        accounts.add_offline("Steve");
        accounts.remove(1);
        assert_eq!(accounts.selected().unwrap().username(), "Main");
        accounts.remove(0);
        assert!(accounts.selected().is_none());
    }

    #[test]
    fn renaming_moves_the_selection_and_stats_and_refuses_taken_names() {
        let mut accounts = Accounts::default();
        accounts.add_offline("Steve");
        accounts.add_offline("Alex");
        accounts.set_locked(0, true);
        accounts.record_play("offline:steve", 60, 1000);
        assert!(!accounts.rename_offline(0, "alex"));
        assert!(!accounts.rename_offline(0, "no good"));
        accounts.select(0);
        assert!(accounts.rename_offline(0, "Herobrine"));
        assert_eq!(accounts.selected().unwrap().username(), "Herobrine");
        let meta = accounts.meta(&accounts.list()[0]);
        assert!(meta.locked);
        assert_eq!((meta.play_secs, meta.last_played), (60, 1000));
        accounts.store(microsoft("Main", "abc"), true);
        assert!(!accounts.rename_offline(2, "Other"));
    }

    #[test]
    fn play_time_adds_up_and_leaves_with_the_account() {
        let mut accounts = Accounts::default();
        accounts.add_offline("Steve");
        accounts.record_play("offline:steve", 30, 5);
        accounts.record_play("offline:steve", 30, 9);
        accounts.record_play("offline:nobody", 30, 9);
        let meta = accounts.meta(&accounts.list()[0]);
        assert_eq!((meta.play_secs, meta.last_played), (60, 9));
        accounts.remove(0);
        assert_eq!(accounts.meta.len(), 0);
    }

    #[test]
    fn offline_uuids_follow_vanilla() {
        assert_eq!(
            offline_uuid("Notch").to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }

    #[test]
    fn the_file_round_trips() {
        let mut accounts = Accounts::default();
        accounts.store(microsoft("Main", "abc"), true);
        accounts.add_offline("Steve");
        let text = serde_json::to_string(&accounts).unwrap();
        let back: Accounts = serde_json::from_str(&text).unwrap();
        assert_eq!(back.list().len(), 2);
        assert!(back.list()[0].is_microsoft());
        assert_eq!(back.selected().unwrap().username(), "Steve");
    }
}
