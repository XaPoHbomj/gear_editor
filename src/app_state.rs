use crate::i18n::Locale;
use axum::http::{HeaderMap, header};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db_path: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) asset_dir: PathBuf,
    pub(crate) dump_dir: PathBuf,
    pub(crate) root_dir: PathBuf,
}

impl AppState {
    pub(crate) fn dump_lang_dir(&self, locale: Locale) -> PathBuf {
        let code = match locale {
            Locale::Ru => "ru",
            Locale::En => "en",
            Locale::Zh => "zh",
            Locale::Ko => "ko",
            Locale::Ja => "ja",
        };
        self.dump_dir.join(code)
    }

    pub(crate) fn read_version(&self) -> String {
        read_version_from_dir(&self.state_dir)
    }
}

pub(crate) fn cookie_value(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let cookie = cookie.trim();
                let (name, value) = cookie.split_once('=')?;
                if name == key {
                    Some(value.to_string())
                } else {
                    None
                }
            })
        })
}

pub(crate) fn read_version_from_dir(state_dir: &Path) -> String {
    let ver_dir = state_dir.join("version");
    let Ok(mut entries) = std::fs::read_dir(&ver_dir) else {
        return String::new();
    };
    let Some(Ok(entry)) = entries.next() else {
        return String::new();
    };
    let name = entry.file_name();
    let name = match name.to_str() {
        Some(n) => n,
        None => return String::new(),
    };
    let start = match name.find(|c: char| c.is_ascii_digit()) {
        Some(i) => i,
        None => return String::new(),
    };
    name[start..].to_string()
}
