//! Who joins: an offline name or a logged-in Microsoft account.

use crate::auth::accounts::Account;

/// Which identity to log in as.
pub enum Identity {
    Offline {
        username: String,
    },
    Online {
        username: String,
        uuid: uuid::Uuid,
        access_token: String,
    },
}

impl Identity {
    /// The account the launcher selected (`accounts.json` in the game folder).
    pub fn saved(game_dir: &std::path::Path) -> Self {
        match crate::auth::load_or_refresh(game_dir) {
            Some(Account::Offline { username }) => return Identity::Offline { username },
            Some(Account::Microsoft(account)) => {
                if let Ok(uuid) = uuid::Uuid::parse_str(&account.uuid) {
                    return Identity::Online {
                        username: account.username,
                        uuid,
                        access_token: account.access_token,
                    };
                }
            }
            None => {}
        }
        let username = std::env::var("MELON_NAME").unwrap_or_else(|_| "Player".to_string());
        Identity::Offline { username }
    }

    pub fn username(&self) -> &str {
        match self {
            Identity::Offline { username } | Identity::Online { username, .. } => username,
        }
    }

    pub fn uuid(&self) -> uuid::Uuid {
        match self {
            Identity::Offline { username } => crate::auth::accounts::offline_uuid(username),
            Identity::Online { uuid, .. } => *uuid,
        }
    }
}
