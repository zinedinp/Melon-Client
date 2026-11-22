//! Resource packs a server sends (vanilla `DownloadedPackSource`).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use sha1::{Digest, Sha1};
use uuid::Uuid;

use super::Pack;

/// `MAX_PACK_SIZE_BYTES`: 250 MiB.
const MAX_PACK_SIZE: u64 = 262_144_000;
/// `DownloadedPackSource.MAX_KEPT_PACKS`: older downloads are deleted.
const MAX_KEPT_PACKS: usize = 20;

/// `ServerData.ServerPackStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Policy {
    Enabled,
    Disabled,
    #[default]
    Prompt,
}

impl Policy {
    /// `acceptTextures` in the server list: missing means prompt.
    pub fn from_accept(accept: Option<bool>) -> Self {
        match accept {
            Some(true) => Self::Enabled,
            Some(false) => Self::Disabled,
            None => Self::Prompt,
        }
    }

    pub fn accept(self) -> Option<bool> {
        match self {
            Self::Enabled => Some(true),
            Self::Disabled => Some(false),
            Self::Prompt => None,
        }
    }

    /// `manageServer.resourcePack.<key>`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::Prompt => "prompt",
        }
    }

    /// The button's next value: enabled, disabled, prompt, enabled...
    pub fn next(self) -> Self {
        match self {
            Self::Enabled => Self::Disabled,
            Self::Disabled => Self::Prompt,
            Self::Prompt => Self::Enabled,
        }
    }
}

/// `ServerboundResourcePackPacket.Action`, by its wire id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    SuccessfullyLoaded = 0,
    Declined = 1,
    FailedDownload = 2,
    Accepted = 3,
    Downloaded = 4,
    InvalidUrl = 5,
    FailedReload = 6,
    Discarded = 7,
}

/// Sends a status for a pack to the server, from any thread.
pub type Responder = Arc<dyn Fn(Uuid, Status) + Send + Sync>;

/// Who is downloading, for the request headers vanilla sends.
#[derive(Clone, Default)]
pub struct User {
    pub name: String,
    pub uuid: Uuid,
}

/// What the game loop should do next.
pub enum Action {
    /// Show `PackConfirmScreen`.
    Prompt(PromptRequest),
    /// The server's packs changed.
    Reload,
    /// Leave the server with this translation key as the reason.
    Disconnect(&'static str),
}

/// The question `PackConfirmScreen` asks.
#[derive(Clone)]
pub struct PromptRequest {
    pub required: bool,
    /// The server's own words.
    pub message: Option<crate::component::Component>,
}

enum State {
    /// Waiting for the player's permission.
    Pending,
    Downloading,
    /// On disk, not loaded yet.
    Downloaded(PathBuf),
    /// In the resource stack.
    Loaded(Arc<Pack>),
    /// Download or load failed; the server was told.
    Failed,
}

struct Entry {
    id: Uuid,
    url: String,
    hash: Option<String>,
    state: State,
    /// Loaded by the reload under way; the server hears how it went.
    reporting: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Permission {
    Pending,
    Allowed,
    Declined,
}

#[derive(Default)]
struct Manager {
    downloads: PathBuf,
    server_address: Option<String>,
    user: User,
    policy: Policy,
    responder: Option<Responder>,
    /// Closes the connection, to leave while still joining.
    close: Option<Box<dyn Fn() + Send>>,
    permission: Option<Permission>,
    packs: Vec<Entry>,
    /// Pushes waiting for the prompt's answer, and whether any is required.
    prompted: Vec<(Uuid, String, Option<String>)>,
    prompt: Option<PromptRequest>,
    prompt_shown: bool,
    /// Packs were added or removed since the last reload.
    dirty: bool,
    disconnect: Option<&'static str>,
}

static MANAGER: Mutex<Option<Manager>> = Mutex::new(None);

fn manager() -> MutexGuard<'static, Option<Manager>> {
    MANAGER.lock().unwrap_or_else(|e| e.into_inner())
}

/// Before joining.
pub fn begin(game_dir: &Path, server_address: Option<String>, policy: Policy, user: User) {
    let mut guard = manager();
    let old = guard.take();
    let dirty =
        old.is_some_and(|m| m.dirty || m.packs.iter().any(|p| matches!(p.state, State::Loaded(_))));
    *guard = Some(Manager {
        downloads: game_dir.join("downloads"),
        server_address,
        user,
        policy,
        permission: Some(match policy {
            Policy::Enabled => Permission::Allowed,
            Policy::Disabled => Permission::Declined,
            Policy::Prompt => Permission::Pending,
        }),
        dirty,
        ..Manager::default()
    });
}

/// The connection is up.
pub fn attach(responder: Responder, close: Option<Box<dyn Fn() + Send>>) {
    if let Some(m) = manager().as_mut() {
        m.responder = Some(responder);
        m.close = close;
    }
}

/// Statuses go out through `responder` from now on.
pub fn set_responder(responder: Responder) {
    if let Some(m) = manager().as_mut() {
        m.responder = Some(responder);
    }
}

/// Closes the connection [`attach`] was given, if any.
pub fn close_connection() {
    if let Some(close) = manager().as_ref().and_then(|m| m.close.as_ref()) {
        close();
    }
}

/// Left the server (`cleanupAfterDisconnect`): its packs come off.
pub fn end() {
    if let Some(m) = manager().as_mut() {
        m.responder = None;
        m.close = None;
        let had_loaded = m.packs.iter().any(|p| matches!(p.state, State::Loaded(_)));
        m.packs.clear();
        m.prompted.clear();
        m.prompt = None;
        m.dirty |= had_loaded;
    }
}

/// `ClientboundResourcePackPushPacket`.
pub fn push(
    id: Uuid,
    url: String,
    hash: String,
    required: bool,
    message: Option<crate::component::Component>,
) {
    let mut guard = manager();
    let Some(m) = guard.as_mut() else {
        return;
    };
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        m.send(id, Status::InvalidUrl);
        return;
    }
    let hash = valid_sha1(&hash);
    let ask = m.policy == Policy::Prompt || (required && m.policy == Policy::Disabled);
    if ask {
        m.prompted.push((id, url, hash));
        let required = required || m.prompt.as_ref().is_some_and(|p| p.required);
        m.prompt = Some(PromptRequest { required, message });
        m.prompt_shown = false;
    } else {
        m.push_pack(id, url, hash);
    }
}

/// `ClientboundResourcePackPopPacket`: one pack, or all of them.
pub fn pop(id: Option<Uuid>) {
    if let Some(m) = manager().as_mut() {
        let before = m.packs.len();
        let mut removed = Vec::new();
        m.packs.retain(|p| {
            let keep = id.is_some_and(|id| p.id != id);
            if !keep {
                removed.push((p.id, matches!(p.state, State::Loaded(_))));
            }
            keep
        });
        for (pack, was_loaded) in removed {
            m.send(pack, Status::Discarded);
            m.dirty |= was_loaded;
        }
        if before != m.packs.len() {
            m.prompted.retain(|(p, ..)| id.is_some_and(|id| *p != id));
        }
    }
}

/// The player answered `PackConfirmScreen`.
pub fn answer_prompt(accept: bool) -> Option<(String, Policy)> {
    let mut guard = manager();
    let m = guard.as_mut()?;
    let required = m
        .prompt
        .take()
        .is_some_and(|p| !m.prompted.is_empty() && p.required);
    m.permission = Some(if accept {
        Permission::Allowed
    } else {
        Permission::Declined
    });
    let policy = if accept {
        Some(Policy::Enabled)
    } else if required {
        m.disconnect = Some("multiplayer.requiredTexturePrompt.disconnect");
        None
    } else {
        Some(Policy::Disabled)
    };
    if let Some(policy) = policy {
        m.policy = policy;
    }
    for (id, url, hash) in std::mem::take(&mut m.prompted) {
        m.push_pack(id, url, hash);
    }
    Some((m.server_address.clone()?, policy?))
}

/// What the game loop should do now, if anything.
pub fn poll() -> Option<Action> {
    let mut guard = manager();
    let m = guard.as_mut()?;
    if let Some(reason) = m.disconnect.take() {
        return Some(Action::Disconnect(reason));
    }
    if let Some(prompt) = &m.prompt
        && !m.prompt_shown
    {
        m.prompt_shown = true;
        return Some(Action::Prompt(prompt.clone()));
    }
    let downloading = m
        .packs
        .iter()
        .any(|p| matches!(p.state, State::Downloading));
    if m.dirty && !downloading {
        m.dirty = false;
        return Some(Action::Reload);
    }
    None
}

/// The packs to stack on top of the player's, lowest first, opened from the download cache.
pub fn active_packs() -> Vec<Arc<Pack>> {
    let mut guard = manager();
    let Some(m) = guard.as_mut() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut failed = Vec::new();
    for entry in &mut m.packs {
        match &entry.state {
            State::Downloaded(path) => {
                match Pack::open(format!("server/{}", entry.id), "server".to_owned(), path) {
                    Ok(pack) => {
                        let pack = Arc::new(pack);
                        out.push(Arc::clone(&pack));
                        entry.state = State::Loaded(pack);
                        entry.reporting = true;
                    }
                    Err(e) => {
                        crate::warn!("Server resource pack {} broken: {e:#}", entry.id);
                        entry.state = State::Failed;
                        failed.push(entry.id);
                    }
                }
            }
            State::Loaded(pack) => out.push(Arc::clone(pack)),
            _ => {}
        }
    }
    for id in failed {
        m.send(id, Status::FailedReload);
    }
    out
}

/// The reload with [`active_packs`] is done.
pub fn reload_finished(ok: bool) {
    let mut guard = manager();
    let Some(m) = guard.as_mut() else {
        return;
    };
    let loaded: Vec<Uuid> = m
        .packs
        .iter_mut()
        .filter_map(|p| std::mem::take(&mut p.reporting).then_some(p.id))
        .collect();
    for id in loaded {
        m.send(
            id,
            if ok {
                Status::SuccessfullyLoaded
            } else {
                Status::FailedReload
            },
        );
    }
}

impl Manager {
    fn send(&self, id: Uuid, status: Status) {
        crate::probe::note("pack.status", format!("{id} {status:?}"), String::new);
        if let Some(responder) = &self.responder {
            responder(id, status);
        }
    }

    /// `ServerPackManager.pushPack`.
    fn push_pack(&mut self, id: Uuid, url: String, hash: Option<String>) {
        crate::probe::note("pack.server", &url, || {
            format!("id {id}, hash {}", hash.as_deref().unwrap_or("(none)"))
        });
        if let Some(i) = self.packs.iter().position(|p| p.id == id) {
            let old = self.packs.remove(i);
            self.dirty |= matches!(old.state, State::Loaded(_));
            self.send(id, Status::Discarded);
        }
        if self.permission == Some(Permission::Declined) {
            self.send(id, Status::Declined);
            return;
        }
        self.packs.push(Entry {
            id,
            url,
            hash,
            state: State::Pending,
            reporting: false,
        });
        if self.permission == Some(Permission::Allowed) {
            self.start_downloads();
        }
    }

    fn start_downloads(&mut self) {
        for entry in &mut self.packs {
            if !matches!(entry.state, State::Pending) {
                continue;
            }
            entry.state = State::Downloading;
            if let Some(responder) = &self.responder {
                responder(entry.id, Status::Accepted);
            }
            let job = Download {
                id: entry.id,
                url: entry.url.clone(),
                hash: entry.hash.clone(),
                dir: self.downloads.join(entry.id.to_string()),
                user: self.user.clone(),
            };
            std::thread::Builder::new()
                .name("pack-download".to_owned())
                .spawn(move || {
                    let result = job.run();
                    finish_download(job.id, &job.url, result);
                })
                .ok();
        }
    }
}

fn finish_download(id: Uuid, url: &str, result: anyhow::Result<PathBuf>) {
    let mut guard = manager();
    let Some(m) = guard.as_mut() else {
        return;
    };
    let Some(entry) = m
        .packs
        .iter_mut()
        .find(|p| p.id == id && p.url == url && matches!(p.state, State::Downloading))
    else {
        return;
    };
    match result {
        Ok(path) => {
            entry.state = State::Downloaded(path);
            m.dirty = true;
            m.send(id, Status::Downloaded);
        }
        Err(e) => {
            crate::warn!("Server resource pack {url} not loaded: {e:#}");
            crate::probe::note("pack.server", format!("{url} FAILED"), || format!("{e:#}"));
            entry.state = State::Failed;
            m.send(id, Status::FailedDownload);
        }
    }
}

struct Download {
    id: Uuid,
    url: String,
    hash: Option<String>,
    dir: PathBuf,
    user: User,
}

impl Download {
    /// `HttpUtil.downloadFile`.
    fn run(&self) -> anyhow::Result<PathBuf> {
        if let Some(hash) = &self.hash {
            let cached = self.dir.join(hash);
            if file_sha1(&cached).ok().as_deref() == Some(hash.as_str()) {
                if let Ok(file) = std::fs::File::options().write(true).open(&cached) {
                    let _ = file.set_modified(std::time::SystemTime::now());
                }
                return Ok(cached);
            }
            let _ = std::fs::remove_file(&cached);
        }
        let (format_major, format_minor) = super::meta::GAME_FORMAT;
        let version = env!("CARGO_PKG_VERSION")
            .split_once("+mc")
            .map_or("unknown", |(_, mc)| mc);
        let response = ureq::get(&self.url)
            .header("X-Minecraft-Username", &self.user.name)
            .header("X-Minecraft-UUID", self.user.uuid.simple().to_string())
            .header("X-Minecraft-Version", version)
            .header("X-Minecraft-Version-ID", version)
            .header(
                "X-Minecraft-Pack-Format",
                format!("{format_major}.{format_minor}"),
            )
            .header("User-Agent", format!("Minecraft Java/{version}"))
            .config()
            .timeout_connect(Some(std::time::Duration::from_secs(20)))
            .build()
            .call()?;
        if let Some(length) = response.body().content_length() {
            anyhow::ensure!(length <= MAX_PACK_SIZE, "Paket zu gross: {length} Bytes");
        }
        std::fs::create_dir_all(&self.dir)?;
        let temp = self
            .dir
            .join(format!("download-{}.tmp", std::process::id()));
        let result = (|| {
            let mut reader = response
                .into_body()
                .into_with_config()
                .limit(MAX_PACK_SIZE + 1)
                .reader();
            let mut file = std::fs::File::create(&temp)?;
            let mut hasher = Sha1::new();
            let mut buf = vec![0u8; 64 * 1024];
            let mut total = 0u64;
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                total += n as u64;
                anyhow::ensure!(total <= MAX_PACK_SIZE, "Paket groesser als 250 MiB");
                hasher.update(&buf[..n]);
                file.write_all(&buf[..n])?;
            }
            file.sync_all()?;
            let actual = hex(&hasher.finalize());
            if let Some(expected) = &self.hash {
                anyhow::ensure!(
                    &actual == expected,
                    "Hash {actual} passt nicht zum angekuendigten {expected}"
                );
            }
            let target = self.dir.join(&actual);
            std::fs::rename(&temp, &target)?;
            Ok(target)
        })();
        let _ = std::fs::remove_file(&temp);
        if let Some(root) = self.dir.parent() {
            vacuum(root, MAX_KEPT_PACKS);
        }
        result
    }
}

/// `DownloadCacheCleaner.vacuumCacheDir`.
fn vacuum(root: &Path, keep: usize) {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .flat_map(|dir| {
            std::fs::read_dir(dir.path())
                .into_iter()
                .flatten()
                .flatten()
        })
        .filter_map(|f| {
            let modified = f.metadata().ok()?.modified().ok()?;
            f.path().is_file().then(|| (modified, f.path()))
        })
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, path) in files.split_off(keep) {
        let _ = std::fs::remove_file(&path);
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

/// A 40-digit hex SHA-1, lowercased; anything else counts as no hash.
fn valid_sha1(hash: &str) -> Option<String> {
    (hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
}

fn file_sha1(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_hashes() {
        assert_eq!(
            valid_sha1("ABCDEF0123456789abcdef0123456789abcdef01").as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef01")
        );
        assert_eq!(valid_sha1(""), None);
        assert_eq!(valid_sha1("xyz"), None);
        let mut hasher = Sha1::new();
        hasher.update(b"abc");
        assert_eq!(
            hex(&hasher.finalize()),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn vacuum_keeps_the_newest() {
        let root = std::env::temp_dir().join(format!("melon-vacuum-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let now = std::time::SystemTime::now();
        for i in 0..5u64 {
            let dir = root.join(format!("pack{i}"));
            std::fs::create_dir_all(&dir).unwrap();
            let file = std::fs::File::create(dir.join("hash")).unwrap();
            file.set_modified(now - std::time::Duration::from_secs(100 - i))
                .unwrap();
        }
        vacuum(&root, 2);
        let left: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(left.len(), 2);
        assert!(root.join("pack4/hash").exists() && root.join("pack3/hash").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn policy_cycles_like_the_button() {
        assert_eq!(Policy::Enabled.next(), Policy::Disabled);
        assert_eq!(Policy::Disabled.next(), Policy::Prompt);
        assert_eq!(Policy::Prompt.next(), Policy::Enabled);
        assert_eq!(Policy::from_accept(None), Policy::Prompt);
    }
}
