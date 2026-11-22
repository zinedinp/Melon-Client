//! Official client JAR SHA-1: Minecraft Wiki `Module:Downloadlinks/hashes`

use serde::Deserialize;

const UA: &str = concat!(
    "Melon/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/zinedinp/Melon-Client)"
);
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const WIKI_HASHES: &str = "https://minecraft.wiki/w/Module:Downloadlinks/hashes?action=raw";
const MOJANG_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Result of comparing a local jar SHA-1 to the official client hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Match,
    Mismatch {
        expected: String,
    },
    /// Version is not in the wiki table or the Mojang manifest.
    Unknown,
    /// No network, or both sources failed.
    Offline,
}

/// Look up the official client SHA-1 for `version` and compare it to `sha1`.
pub fn verify(version: &str, sha1: &str) -> Verdict {
    match official_client_sha1(version) {
        Ok(Some(expected)) if expected.eq_ignore_ascii_case(sha1) => Verdict::Match,
        Ok(Some(expected)) => Verdict::Mismatch { expected },
        Ok(None) => Verdict::Unknown,
        Err(_) => Verdict::Offline,
    }
}

fn official_client_sha1(version: &str) -> Result<Option<String>, String> {
    if let Ok(lua) = get_text(WIKI_HASHES)
        && let Some(hash) = lua_client_hash(&lua, version)
    {
        return Ok(Some(hash));
    }
    mojang_client_sha1(version)
}

fn mojang_client_sha1(version: &str) -> Result<Option<String>, String> {
    let manifest: Manifest = get_json(MOJANG_MANIFEST)?;
    let Some(entry) = manifest
        .versions
        .iter()
        .find(|v| v.id.eq_ignore_ascii_case(version))
    else {
        return Ok(None);
    };
    let file: VersionFile = get_json(&entry.url)?;
    let hash = file.downloads.client.sha1.to_ascii_lowercase();
    if hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(Some(hash))
    } else {
        Ok(None)
    }
}

fn get_text(url: &str) -> Result<String, String> {
    let mut response = ureq::get(url)
        .header("User-Agent", UA)
        .config()
        .timeout_global(Some(TIMEOUT))
        .build()
        .call()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("http {}", response.status()));
    }
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}

fn get_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let text = get_text(url)?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// `client = '…'` (or `"…"`) inside the `['version'] = { … }` wiki table entry.
pub fn lua_client_hash(lua: &str, version: &str) -> Option<String> {
    let key = version.to_ascii_lowercase();
    let patterns = [format!("['{key}']"), format!("[\"{key}\"]")];
    let start = patterns.iter().find_map(|p| lua.find(p.as_str()))?;
    let block = lua.get(start..start.saturating_add(512).min(lua.len()))?;
    let client = block.find("client")?;
    let after = block.get(client + "client".len()..)?;
    let eq = after.find('=')?;
    let rest = after.get(eq + 1..)?.trim_start();
    let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = rest.get(1..)?;
    let end = inner.find(quote)?;
    let hash = inner.get(..end)?.trim();
    (hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
}

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<ManifestVersion>,
}

#[derive(Deserialize)]
struct ManifestVersion {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct VersionFile {
    downloads: Downloads,
}

#[derive(Deserialize)]
struct Downloads {
    client: Artifact,
}

#[derive(Deserialize)]
struct Artifact {
    sha1: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
return {
	['19w12b'] = {
		client = '2fc0501773a90d120c9f6476037c4bfba554a549',
		server = '37d6d9753b8eac2420e9deba132c38e00c8204c3',
		json_package = '22b64304b7d1e19325c8db612631db200776d77d'
    },
    ['1.13.1-pre2'] = {
		client = '7eaeaa37e0c7642d519c39de70a630119ad4929a',
		server = 'c2a4bcf3e244c46f13c39e31e7ef6030564fb6c2',
		json = '4f2100e24eb32ab94447de5808b96fdade6badd3',
		json_package = '1594d6f7242d2065c2b61c242397f40677e5123c'
    },
}
"#;

    #[test]
    fn wiki_lua_yields_the_client_hash() {
        assert_eq!(
            lua_client_hash(SAMPLE, "1.13.1-pre2").as_deref(),
            Some("7eaeaa37e0c7642d519c39de70a630119ad4929a")
        );
        assert_eq!(
            lua_client_hash(SAMPLE, "19w12b").as_deref(),
            Some("2fc0501773a90d120c9f6476037c4bfba554a549")
        );
        assert_eq!(lua_client_hash(SAMPLE, "26.3"), None);
    }
}
