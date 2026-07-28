use crate::i18n::Locale;
use axum::http::{HeaderMap, header};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) state_dir: PathBuf,
    pub(crate) prod_state_dir: PathBuf,
    pub(crate) asset_dir: PathBuf,
    pub(crate) prod_asset_dir: PathBuf,
    pub(crate) dump_dir: PathBuf,
    pub(crate) root_dir: PathBuf,
    pub(crate) ctl_addr: String,
    pub(crate) prod_ctl_addr: String,
    pub(crate) passwd_path: PathBuf,
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

    pub(crate) fn dump_file(
        &self,
        locale: Locale,
        filename: impl AsRef<::std::path::Path>,
    ) -> PathBuf {
        let lang_dir = self.dump_lang_dir(locale);
        let path = lang_dir.join(&filename);
        if path.exists() {
            return path;
        }
        if locale == Locale::Ru {
            let en_path = self.dump_dir.join("en").join(&filename);
            if en_path.exists() {
                return en_path;
            }
        }
        path
    }

    pub(crate) fn read_version(&self, prod: bool) -> String {
        let dir = if prod {
            &self.prod_state_dir
        } else {
            &self.state_dir
        };
        read_version_from_dir(dir)
    }

    pub(crate) fn active_ctl_addr(&self, headers: &HeaderMap) -> &str {
        if active_server_mode(headers) == ServerMode::Prod {
            &self.prod_ctl_addr
        } else {
            &self.ctl_addr
        }
    }
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
    let mut version = name[start..].to_string();
    if let Some(dot) = version.rfind('.') {
        version.truncate(dot);
    }
    version
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerMode {
    Beta,
    Prod,
}

pub(crate) fn parse_server_mode(value: &str) -> ServerMode {
    if value.eq_ignore_ascii_case("prod") {
        ServerMode::Prod
    } else {
        ServerMode::Beta
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

pub(crate) fn active_server_mode(headers: &HeaderMap) -> ServerMode {
    let value = cookie_value(headers, "gear_server").unwrap_or_else(|| "beta".to_string());
    parse_server_mode(&value)
}

pub(crate) fn state_with_active_server(state: &AppState, headers: &HeaderMap) -> AppState {
    if active_server_mode(headers) == ServerMode::Prod {
        let mut active = state.clone();
        active.state_dir = active.prod_state_dir.clone();
        if active.prod_asset_dir.exists() {
            active.asset_dir = active.prod_asset_dir.clone();
        }
        active.ctl_addr = active.prod_ctl_addr.clone();
        active
    } else {
        state.clone()
    }
}
