use axum::http::{HeaderMap, header};
use std::path::PathBuf;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db_path: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) prod_state_dir: PathBuf,
    pub(crate) asset_dir: PathBuf,
    pub(crate) prod_asset_dir: PathBuf,
    pub(crate) dump_dir: PathBuf,
    pub(crate) root_dir: PathBuf,
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
    let mut active = state.clone();
    if active_server_mode(headers) == ServerMode::Prod {
        active.state_dir = active.prod_state_dir.clone();
        if active.prod_asset_dir.exists() {
            active.asset_dir = active.prod_asset_dir.clone();
        }
    }
    active
}
