//! Vanilla `Component.translatable` / `LanguageManager`.

pub use melon_base::i18n::languages;

use crate::assets::Assets;
use crate::i18n::languages::Language;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `LanguageManager.getLanguages()`.
pub fn available_languages(assets: &Assets) -> Vec<Language> {
    let mut languages: Vec<Language> = Language::ALL
        .iter()
        .copied()
        .filter(|l| {
            *l == Language::EnUs
                || crate::assets::resource_exists(assets.root(), "lang", l.code(), "json")
        })
        .collect();
    languages.sort_by_key(|l| l.code());
    languages
}

/// Key to translation.
type Strings = HashMap<Box<str>, Box<str>>;

pub struct Lang {
    /// Selected language (`LanguageSelectScreen` / `languageManager.getSelected()`).
    pub language: Language,
    /// Translations as the files have them, placeholders and `%%` intact.
    strings: Strings,
    /// The translations that contain `%%`, with it unescaped once at load.
    unescaped: Strings,
}

/// Baked into the launcher (and the game overlay) so a published binary has English and German
/// without assets.
const EMBEDDED_EN: &str = include_str!("../../lang/en_us.json");
const EMBEDDED_DE: &str = include_str!("../../lang/de_de.json");

fn project_lang_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("lang"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("lang")))
            .unwrap_or_else(|| PathBuf::from("lang")),
    ];
    candidates
        .into_iter()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("lang"))
}

fn embedded_lang(code: &str) -> Option<Strings> {
    match code {
        "en_us" => parse_lang(EMBEDDED_EN.as_bytes()),
        "de_de" => parse_lang(EMBEDDED_DE.as_bytes()),
        _ => None,
    }
}

fn load_json_lang(path: &Path) -> Option<Strings> {
    parse_lang(&std::fs::read(path).ok()?)
}

/// Embedded strings, then a checkout `lang/` file on top when present.
fn project_lang(code: &str) -> Option<Strings> {
    let mut strings = embedded_lang(code)?;
    if let Some(overlay) = load_json_lang(&project_lang_dir().join(format!("{code}.json"))) {
        strings.extend(overlay);
    }
    Some(strings)
}

fn parse_lang(bytes: &[u8]) -> Option<Strings> {
    let value: serde_json::Value =
        serde_json::from_slice(crate::assets::packs::meta::strip_bom(bytes)).ok()?;
    let object = value.as_object()?;
    Some(
        object
            .iter()
            .filter(|(k, _)| *k != "_comment")
            .filter_map(|(k, v)| v.as_str().map(|s| (k.as_str().into(), s.into())))
            .collect(),
    )
}

/// `ClientLanguage.loadFrom` for one language.
fn load_lang_stack(assets: &Assets, code: &str, strings: &mut Strings) {
    for namespace in crate::assets::namespaces(assets.root()) {
        let copies =
            crate::assets::resource_stack(assets.root(), &namespace, &format!("lang/{code}.json"));
        for bytes in copies {
            if let Some(overlay) = parse_lang(&bytes) {
                strings.extend(overlay);
            }
        }
    }
}

/// Zip-mod `assets/<ns>/lang/<code>.json`, even when pack discovery misses a mod.
fn load_extracted_mod_langs(assets: &Assets, code: &str, strings: &mut Strings) {
    for dir in pumpkin_mod::extracted_mods(&assets.game_dir().join("mods").join(".cache")) {
        merge_extracted_lang_dir(&dir, code, strings);
    }
}

fn merge_extracted_lang_dir(dir: &Path, code: &str, strings: &mut Strings) {
    let assets_root = dir.join("assets");
    let Ok(namespaces) = std::fs::read_dir(&assets_root) else {
        return;
    };
    for ns in namespaces.flatten() {
        let path = ns.path().join("lang").join(format!("{code}.json"));
        if let Some(overlay) = load_json_lang(&path) {
            strings.extend(overlay);
        }
    }
}

impl Lang {
    /// Melon overlay only: baked `lang/*.json`, no Mojang files required.
    pub fn launcher(language: Language) -> Self {
        let mut strings = HashMap::new();
        if let Some(overlay) = project_lang("en_us") {
            strings.extend(overlay);
        }
        if language != Language::EnUs
            && let Some(overlay) = project_lang(language.code())
        {
            strings.extend(overlay);
        }
        Self::from_strings(language, strings)
    }

    /// `en_us` baseline, then `code` on top. Vanilla English fallback for missing keys.
    pub fn load(assets: &Assets, language: Language) -> Self {
        let mut strings = HashMap::new();
        load_lang_stack(assets, "en_us", &mut strings);
        if let Some(overlay) = project_lang("en_us") {
            strings.extend(overlay);
        }
        load_extracted_mod_langs(assets, "en_us", &mut strings);
        if language != Language::EnUs {
            let code = language.code();
            load_lang_stack(assets, code, &mut strings);
            if let Some(overlay) = project_lang(code) {
                strings.extend(overlay);
            }
            load_extracted_mod_langs(assets, code, &mut strings);
        }
        Self::from_strings(language, strings)
    }

    fn from_strings(language: Language, mut strings: Strings) -> Self {
        strings.shrink_to_fit();
        let unescaped = strings
            .iter()
            .filter(|(_, v)| v.contains("%%"))
            .map(|(k, v)| (k.clone(), v.replace("%%", "%").into_boxed_str()))
            .collect();
        Self {
            language,
            strings,
            unescaped,
        }
    }

    /// `Component.translatable` runs `String.format` even with no args, so `%%` to `%`
    pub fn get(&self, key: &'static str) -> &str {
        self.get_dynamic(key)
    }

    /// [`get`](Self::get) for a key built at runtime.
    pub fn get_dynamic<'a>(&'a self, key: &'a str) -> &'a str {
        match self.unescaped.get(key) {
            Some(s) => s,
            None => self.raw(key),
        }
    }

    /// `Component.translatable(key, args)`: the translation with its `%s`
    pub fn format(&self, key: &'static str, args: &[&str]) -> String {
        fill(self.raw(key), args)
    }

    /// [`format`](Self::format) for a key built at runtime.
    pub fn format_dynamic(&self, key: &str, args: &[&str]) -> String {
        fill(self.raw(key), args)
    }

    fn raw<'a>(&'a self, key: &'a str) -> &'a str {
        self.strings.get(key).map_or(key, |s| s)
    }

    /// The untranslated pattern of `key`, placeholders and `%%` as they are.
    pub fn raw_dynamic(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(|s| &**s)
    }

    /// Whether `key` has a translation.
    pub fn has(&self, key: &str) -> bool {
        self.strings.contains_key(key)
    }

    /// A language with no strings, for tests.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self::from_strings(Language::EnUs, HashMap::new())
    }

    /// A language with just these strings, for tests.
    #[cfg(test)]
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self::from_strings(
            Language::EnUs,
            pairs
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
        )
    }
}

/// `String.format` for the two placeholders translations use.
fn fill(pattern: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut next = 0;
    let mut rest = pattern;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        if let Some(after) = rest.strip_prefix('%') {
            out.push('%');
            rest = after;
        } else if let Some(after) = rest.strip_prefix('s') {
            out.push_str(args.get(next).copied().unwrap_or(""));
            next += 1;
            rest = after;
        } else if let Some((index, after)) = rest
            .split_once("$s")
            .and_then(|(n, after)| Some((n.parse::<usize>().ok()?, after)))
        {
            out.push_str(args.get(index.wrapping_sub(1)).copied().unwrap_or(""));
            rest = after;
        } else {
            out.push('%');
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::languages::Language;

    #[test]
    fn launcher_strings_are_baked_in() {
        let lang = Lang::launcher(Language::EnUs);
        assert_eq!(lang.get("melon.launcher.play"), "Play");
        assert_eq!(lang.get("melon.launcher.download"), "Download");
        assert_eq!(
            lang.get("melon.launcher.setting.global"),
            "Global options: %s"
        );
        assert_eq!(
            lang.get("melon.launcher.download.title"),
            "Downloading Melon"
        );
        let de = Lang::launcher(Language::DeDe);
        assert_eq!(de.get("melon.launcher.play"), "Spielen");
        assert_eq!(de.get("melon.launcher.download"), "Herunterladen");
    }

    #[test]
    fn fills_placeholders_like_string_format() {
        assert_eq!(fill("%s: %s", &["FOV", "Normal"]), "FOV: Normal");
        assert_eq!(
            fill("%s: %s%%", &["Entity Distance", "100"]),
            "Entity Distance: 100%"
        );
        assert_eq!(fill("%2$s %1$s", &["a", "b"]), "b a");
        assert_eq!(fill("%s Chunks", &[]), " Chunks");
        assert_eq!(fill("100% sure", &[]), "100% sure");
    }

    #[test]
    fn extracted_mod_lang_translates_industrial_tab() {
        let dir = std::env::temp_dir().join(format!(
            "melon-mod-lang-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let lang_dir = dir.join("assets/hello/lang");
        std::fs::create_dir_all(&lang_dir).unwrap();
        std::fs::write(
            lang_dir.join("en_us.json"),
            r#"{"itemGroup.industrial":"Industrial"}"#,
        )
        .unwrap();
        let mut strings = HashMap::new();
        merge_extracted_lang_dir(&dir, "en_us", &mut strings);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            strings.get("itemGroup.industrial").map(|s| &**s),
            Some("Industrial")
        );
    }
}
