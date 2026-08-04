use crate::i18n::Locale;
use axum::http::{HeaderMap, header};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) struct AppState {
    /// Base dir for beta per-server saves: <root>/bin_remielle
    pub(crate) state_dir: PathBuf,
    /// Base dir for prod per-server saves: <root>/bin_remielle_prod
    pub(crate) prod_state_dir: PathBuf,
    pub(crate) asset_dir: PathBuf,
    pub(crate) prod_asset_dir: PathBuf,
    /// Data dump dir for beta (zzz_dump/latest).
    pub(crate) dump_dir: PathBuf,
    /// Data dump dir for prod (zzz_dump/live).
    pub(crate) live_dump_dir: PathBuf,
    pub(crate) root_dir: PathBuf,
    /// Base beta ctl address (server 1). e.g. 127.0.0.1:15811
    pub(crate) ctl_addr: String,
    /// Base prod ctl address (server 1). e.g. 127.0.0.1:15911
    pub(crate) prod_ctl_addr: String,
    pub(crate) passwd_path: PathBuf,
    /// base_player_uid for the currently selected server (resolved per selection).
    pub(crate) base_player_uid: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ServerSelection {
    pub(crate) is_prod: bool,
    pub(crate) server_num: u32,
}

impl ServerSelection {
    pub(crate) fn label(&self) -> String {
        format!(
            "{} {}",
            if self.is_prod { "Prod" } else { "Beta" },
            self.server_num
        )
    }

    pub(crate) fn cookie_value(&self) -> String {
        format!(
            "{}:{}",
            if self.is_prod { "prod" } else { "beta" },
            self.server_num
        )
    }
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

    pub(crate) fn server_state_dir(&self, is_prod: bool, server_num: u32) -> PathBuf {
        let base = if is_prod {
            &self.prod_state_dir
        } else {
            &self.state_dir
        };
        base.join(format!("server{server_num}"))
            .join("Persistent/LocalStorage")
    }

    pub(crate) fn server_ctl_addr(&self, is_prod: bool, server_num: u32) -> String {
        let base = if is_prod {
            &self.prod_ctl_addr
        } else {
            &self.ctl_addr
        };
        match base.rsplit_once(':') {
            Some((host, port_str)) => {
                let base_port = port_str.parse::<u32>().unwrap_or(0);
                format!("{}:{}", host, base_port + server_num - 1)
            }
            None => base.clone(),
        }
    }

    /// Base player UID from the per-server config.zon (base_player_uid field).
    pub(crate) fn server_base_uid(&self, is_prod: bool, server_num: u32) -> u32 {
        let configs_dir = if is_prod {
            self.root_dir.join("configs_remielle_prod")
        } else {
            self.root_dir.join("configs_remielle")
        };
        let path = configs_dir.join(format!("server{server_num}/config.zon"));
        let Ok(data) = std::fs::read_to_string(&path) else {
            return 1;
        };
        let Some(line) = data
            .lines()
            .find(|l| l.trim_start().starts_with(".base_player_uid"))
        else {
            return 1;
        };
        Self::parse_base_player_uid(line)
    }

    /// Parse the `.base_player_uid` value out of a config.zon line like
    /// `    .base_player_uid = 400,`.
    pub(crate) fn parse_base_player_uid(line: &str) -> u32 {
        line.trim()
            .trim_start_matches(".base_player_uid")
            .trim()
            .trim_start_matches('=')
            .trim()
            .trim_end_matches(',')
            .trim()
            .parse::<u32>()
            .unwrap_or(1)
    }

    /// Address of the ctl port for the currently selected server. The caller
    /// must pass a state already scoped via `state_with_active_server` /
    /// `state_for_selected_server`, which sets `ctl_addr` to the resolved port.
    pub(crate) fn active_ctl_addr(&self, _headers: &HeaderMap) -> String {
        self.ctl_addr.clone()
    }

    pub(crate) fn read_version(&self, sel: ServerSelection) -> String {
        read_version_from_dir(&self.server_state_dir(sel.is_prod, sel.server_num))
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

pub(crate) fn parse_server_selection(value: &str) -> ServerSelection {
    let (env, num) = match value.split_once(':') {
        Some((e, n)) => (e, n.parse::<u32>().unwrap_or(1)),
        None => (value, 1),
    };
    let is_prod = env.eq_ignore_ascii_case("prod");
    // Beta is consolidated to a single server (server1); prod keeps 3 servers.
    let server_num = if is_prod { num.clamp(1, 3) } else { num.clamp(1, 1) };
    ServerSelection {
        is_prod,
        server_num,
    }
}

pub(crate) fn active_server_selection(headers: &HeaderMap) -> ServerSelection {
    let value = cookie_value(headers, "gear_server").unwrap_or_else(|| "beta:1".to_string());
    parse_server_selection(&value)
}

pub(crate) fn state_with_active_server(state: &AppState, headers: &HeaderMap) -> AppState {
    state_for_selected_server(state, active_server_selection(headers))
}

pub(crate) fn state_for_selected_server(state: &AppState, sel: ServerSelection) -> AppState {
    let mut active = state.clone();
    let dir = active.server_state_dir(sel.is_prod, sel.server_num);
    let ctl = active.server_ctl_addr(sel.is_prod, sel.server_num);
    active.state_dir = dir.clone();
    active.prod_state_dir = dir;
    active.ctl_addr = ctl.clone();
    active.prod_ctl_addr = ctl;
    active.base_player_uid = active.server_base_uid(sel.is_prod, sel.server_num);
    if sel.is_prod {
        if active.prod_asset_dir.exists() {
            active.asset_dir = active.prod_asset_dir.clone();
        }
        if active.live_dump_dir.exists() {
            active.dump_dir = active.live_dump_dir.clone();
        }
    }
    active
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_state() -> AppState {
        AppState {
            state_dir: PathBuf::from("/tmp/zzz/bin_remielle"),
            prod_state_dir: PathBuf::from("/tmp/zzz/bin_remielle_prod"),
            asset_dir: PathBuf::from("/tmp/zzz/remielle/assets/filecfg"),
            prod_asset_dir: PathBuf::from("/tmp/zzz/remielle_prod/assets/filecfg"),
            dump_dir: PathBuf::from("/tmp/zzz/zzz_dump/latest"),
            live_dump_dir: PathBuf::from("/tmp/zzz/zzz_dump/live"),
            root_dir: PathBuf::from("/tmp/zzz"),
            ctl_addr: "127.0.0.1:15811".to_string(),
            prod_ctl_addr: "127.0.0.1:15911".to_string(),
            passwd_path: PathBuf::from("/tmp/zzz/remielle/Persistent/SDK/passwd"),
            base_player_uid: 1,
        }
    }

    #[test]
    fn server_state_dir_resolves_per_server() {
        let s = test_state();
        assert_eq!(
            s.server_state_dir(false, 2),
            PathBuf::from("/tmp/zzz/bin_remielle/server2/Persistent/LocalStorage")
        );
        assert_eq!(
            s.server_state_dir(true, 3),
            PathBuf::from("/tmp/zzz/bin_remielle_prod/server3/Persistent/LocalStorage")
        );
    }

    #[test]
    fn server_ctl_addr_resolves_ports() {
        let s = test_state();
        assert_eq!(s.server_ctl_addr(false, 1), "127.0.0.1:15811");
        assert_eq!(s.server_ctl_addr(false, 2), "127.0.0.1:15812");
        assert_eq!(s.server_ctl_addr(true, 1), "127.0.0.1:15911");
        assert_eq!(s.server_ctl_addr(true, 3), "127.0.0.1:15913");
    }

    #[test]
    fn active_ctl_addr_not_double_incremented() {
        // prod:3 active state (fresh base each time, as in the router)
        let base = test_state();
        let s = state_for_selected_server(&base, ServerSelection { is_prod: true, server_num: 3 });
        assert_eq!(s.active_ctl_addr(&HeaderMap::new()), "127.0.0.1:15913");
        // beta:2 active state
        let s = state_for_selected_server(&base, ServerSelection { is_prod: false, server_num: 2 });
        assert_eq!(s.active_ctl_addr(&HeaderMap::new()), "127.0.0.1:15812");
    }

    #[test]
    fn parses_base_player_uid_line() {
        assert_eq!(
            AppState::parse_base_player_uid("    .base_player_uid = 400,"),
            400
        );
        assert_eq!(
            AppState::parse_base_player_uid(".base_player_uid = 100,"),
            100
        );
        assert_eq!(AppState::parse_base_player_uid(".base_player_uid=55,"), 55);
        // Missing field -> default 1
        assert_eq!(AppState::parse_base_player_uid(".game_bind_address = ..."), 1);
    }

    #[test]
    fn parse_server_selection_handles_new_format() {
        let s = parse_server_selection("beta:2"); // beta consolidates to 1
        assert!(!s.is_prod);
        assert_eq!(s.server_num, 1);
        let s = parse_server_selection("prod:3");
        assert!(s.is_prod);
        assert_eq!(s.server_num, 3);
        let s = parse_server_selection("prod"); // legacy
        assert!(s.is_prod);
        assert_eq!(s.server_num, 1);
        let s = parse_server_selection("prod:9"); // clamp
        assert_eq!(s.server_num, 3);
        let s = parse_server_selection("beta:9"); // beta clamp to 1
        assert!(!s.is_prod);
        assert_eq!(s.server_num, 1);
    }

    #[test]
    fn dump_dir_resolves_per_env() {
        let mut s = test_state();
        // Create the live dir so the prod selection switches to it.
        std::fs::create_dir_all("/tmp/zzz/zzz_dump/live").unwrap();
        s.live_dump_dir = PathBuf::from("/tmp/zzz/zzz_dump/live");
        let beta = state_for_selected_server(
            &s,
            ServerSelection {
                is_prod: false,
                server_num: 1,
            },
        );
        assert_eq!(beta.dump_dir, PathBuf::from("/tmp/zzz/zzz_dump/latest"));
        let prod = state_for_selected_server(
            &s,
            ServerSelection {
                is_prod: true,
                server_num: 2,
            },
        );
        assert_eq!(prod.dump_dir, PathBuf::from("/tmp/zzz/zzz_dump/live"));
        // Cleanup temp dirs created by this test
        let _ = std::fs::remove_dir_all("/tmp/zzz/zzz_dump");
    }

    #[test]
    fn selection_label_and_cookie() {
        let s = ServerSelection {
            is_prod: false,
            server_num: 3,
        };
        assert_eq!(s.label(), "Beta 3");
        assert_eq!(s.cookie_value(), "beta:3");
        let s = ServerSelection {
            is_prod: true,
            server_num: 1,
        };
        assert_eq!(s.label(), "Prod 1");
        assert_eq!(s.cookie_value(), "prod:1");
    }
}
