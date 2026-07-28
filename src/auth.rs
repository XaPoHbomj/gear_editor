use crate::app_state::{AppState, cookie_value};
use axum::{
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use rand::{Rng, distributions::Alphanumeric};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

pub(crate) const ADMIN_LOGIN: &str = "XaPoHbomj";

static SESSION_STORE: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();

pub(crate) fn insert_session(session: Session) -> String {
    let session_id = new_session_id();
    let store = SESSION_STORE.get_or_init(|| Mutex::new(HashMap::new()));
    store.lock().unwrap().insert(session_id.clone(), session);
    session_id
}

#[derive(Clone)]
pub(crate) struct Session {
    pub(crate) uid: i32,
    pub(crate) username: String,
    pub(crate) pending_writes: HashMap<PathBuf, String>,
    pub(crate) last_active: Instant,
}

pub(crate) fn validate_login(
    state: &AppState,
    username: &str,
    password: &str,
) -> Result<Option<(Session, bool)>, String> {
    let data = std::fs::read(&state.passwd_path).map_err(|e| format!("read passwd: {e}"))?;
    if data.len() < 8 {
        return Err("invalid passwd file".into());
    }
    let count = u64::from_le_bytes(data[0..8].try_into().unwrap()) as usize;
    let expected = 8 + count * (32 + 64 + 257);
    if data.len() < expected {
        return Err("truncated passwd file".into());
    }
    let mut pos = 8;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let end = data[pos..]
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(32)
            .min(32);
        names.push(
            std::str::from_utf8(&data[pos..pos + end])
                .unwrap_or("")
                .to_string(),
        );
        pos += 32;
    }
    pos += count * 64; // skip tokens
    let mut found_uid = None;
    for i in 0..count {
        let hash_end = data[pos..]
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(257)
            .min(257);
        let hash_str = std::str::from_utf8(&data[pos..pos + hash_end]).unwrap_or("");
        if names[i] == username {
            if !hash_str.is_empty() && bcrypt::verify(password, hash_str).unwrap_or(false) {
                found_uid = Some(i as i32 + 1);
            }
            break;
        }
        pos += 257;
    }
    let is_admin = username == ADMIN_LOGIN;
    match found_uid {
        Some(uid) => Ok(Some((
            Session {
                uid,
                username: username.to_string(),
                pending_writes: HashMap::new(),
                last_active: Instant::now(),
            },
            is_admin,
        ))),
        None => Ok(None),
    }
}

pub(crate) fn new_session_id() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect()
}

pub(crate) fn get_session(headers: &HeaderMap) -> Option<(String, Session)> {
    let session_id = cookie_value(headers, "ge_session")?;

    let store = SESSION_STORE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut sessions = store.lock().unwrap();
    if let Some(mut session) = sessions.get(&session_id).cloned() {
        session.last_active = Instant::now();
        sessions.insert(session_id.clone(), session.clone());
        Some((session_id, session))
    } else {
        None
    }
}

pub(crate) fn get_session_mut(headers: &HeaderMap) -> Option<(String, Session)> {
    get_session(headers)
}

pub(crate) fn set_session(session_id: String, session: Session) {
    let store = SESSION_STORE.get_or_init(|| Mutex::new(HashMap::new()));
    store.lock().unwrap().insert(session_id, session);
}

pub(crate) fn sanitize_next_path(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed.starts_with('/') || trimmed.starts_with("//") {
        return None;
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return None;
    }
    Some(trimmed.to_string())
}

pub(crate) fn url_encode_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

pub(crate) fn html_escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn html_escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn gc_sessions(max_age: Duration) -> usize {
    let store = SESSION_STORE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut sessions = store.lock().unwrap();
    let before = sessions.len();
    sessions.retain(|_, s| s.last_active.elapsed() < max_age);

    before - sessions.len()
}

pub(crate) fn redirect_to_login(original_uri: &axum::http::Uri) -> Response {
    let attempted = original_uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/dashboard");
    let next = sanitize_next_path(attempted).unwrap_or_else(|| "/dashboard".to_string());
    let location = format!("/?next={}", url_encode_component(&next));
    Redirect::to(&location).into_response()
}

pub(crate) fn remove_session(session_id: &str) {
    let store = SESSION_STORE.get_or_init(|| Mutex::new(HashMap::new()));
    store.lock().unwrap().remove(session_id);
}

pub(crate) fn is_admin(session: &Session) -> bool {
    session.username == ADMIN_LOGIN
}
