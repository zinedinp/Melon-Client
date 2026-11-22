//! Who joins: an offline name, or an account signed in with Microsoft or another service.

use crate::auth::accounts::Account;
use crate::auth::skin::{self, SkinSource};

/// Which identity to log in as.
pub enum Identity {
    Offline {
        username: String,
    },
    Online {
        username: String,
        uuid: uuid::Uuid,
        access_token: String,
        /// The session server joins go to: Mojang's, or the sign-in service's.
        session: String,
    },
}

impl Identity {
    /// The account the launcher selected (`accounts.json` in the game folder). Skins come from
    /// its service from now on.
    pub fn saved(game_dir: &std::path::Path) -> Self {
        let account = crate::auth::load_or_refresh(game_dir);
        skin::set_source(match account.as_ref().and_then(Account::provider) {
            Some(provider) => SkinSource::of(provider),
            None => SkinSource::mojang(),
        });
        match account {
            Some(Account::Offline { username }) => return Identity::Offline { username },
            Some(Account::Microsoft(account)) => {
                if let Ok(uuid) = uuid::Uuid::parse_str(&account.uuid) {
                    return Identity::Online {
                        username: account.username,
                        uuid,
                        access_token: account.access_token,
                        session: crate::auth::minecraft::MOJANG_SESSION.to_owned(),
                    };
                }
            }
            Some(Account::Alternative(account)) => {
                if let Ok(uuid) = uuid::Uuid::parse_str(&account.uuid) {
                    return Identity::Online {
                        username: account.username,
                        uuid,
                        access_token: account.access_token,
                        session: account.provider.session_base(),
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
