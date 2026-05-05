use crate::{
    auth::{get_session_mut, redirect_to_login, set_session},
    zon::format_zon_pretty,
};
use axum::{
    extract::OriginalUri,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use std::fs;

pub(crate) async fn apply_changes(
    headers: HeaderMap,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    for (path, content) in session.pending_writes.drain() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let formatted = format_zon_pretty(&content);
        if let Err(err) = fs::write(&path, formatted) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!("Failed to write {}: {}", path.display(), err)),
            )
                .into_response();
        }
    }

    set_session(session_id, session);
    Redirect::to("/dashboard").into_response()
}

pub(crate) fn svg_data_uri(label: &str) -> String {
    let mut safe = label
        .replace('%', "%25")
        .replace('#', "%23")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('"', "%22")
        .replace(' ', "%20");

    if safe.len() > 32 {
        safe.truncate(32);
    }

    format!(
        "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='320' height='180'><rect width='100%25' height='100%25' fill='%23131a24'/><text x='50%25' y='50%25' dominant-baseline='middle' text-anchor='middle' fill='%239aa4b2' font-size='14' font-family='sans-serif'>{}</text></svg>",
        safe
    )
}
