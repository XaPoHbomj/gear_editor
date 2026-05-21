use crate::{
    AppState,
    auth::{get_session_mut, redirect_to_login, set_session},
    zon::format_zon_pretty,
};
use axum::{
    extract::{OriginalUri, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use std::{fs, path::Path};

pub(crate) fn audit_log(root_dir: &Path, username: &str, uid: i32, action: &str, detail: &str) {
    let log_dir = root_dir.join("logs");
    let _ = fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("gear_editor_audit.log");

    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let escaped_detail = detail.replace('\\', "\\\\").replace('"', "\\\"");

    let line = format!(
        r#"{{"ts":"{}","user":"{}","uid":{},"action":"{}","detail":"{}"}}
"#,
        ts, username, uid, action, escaped_detail
    );

    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }
}

pub(crate) fn shared_page_css() -> &'static str {
    "body{font-family:system-ui,sans-serif;margin:0;background:#0f1115;color:#e6e6e6}\
     .container{padding:24px;max-width:900px;margin:0 auto}\
     input,select{width:100%;box-sizing:border-box;padding:8px;border-radius:8px;border:1px solid #2a3140;background:#121620;color:#e6e6e6}\
     label{display:block;margin:12px 0 6px;font-size:12px;color:#9aa4b2}\
     button{margin-top:16px;padding:10px 14px;border:0;border-radius:8px;background:#4c7dff;color:#fff;font-weight:600;cursor:pointer}\
     .row{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}\
     .row>*{min-width:0}\
     .hero{display:flex;gap:16px;align-items:center;margin-bottom:16px}\
     .hero img{width:120px;height:120px;border-radius:12px;object-fit:cover;object-position:top;border:1px solid #2a3140;background:#0f1115}\
     .hero h1{margin:0}\
.meta{color:#9aa4b2;font-size:12px}\
      .back{display:inline-flex;align-items:center;justify-content:center;padding:10px 14px;border-radius:8px;background:#2a3140;color:#e6e6e6;text-decoration:none;font-weight:600}\
      .form-actions{display:flex;justify-content:space-between;gap:12px;margin-top:16px}\
      .form-actions button{margin-top:0}\
      .preview-img{display:none;width:33.33%;aspect-ratio:1/1;object-fit:contain;border-radius:8px;border:1px solid #2a3140;background:#0f1115;margin:0 0 8px}\
      @media(max-width:768px){.container{padding:14px}.row{grid-template-columns:1fr}button{width:100%}.hero{flex-direction:column;align-items:flex-start}.hero img{width:100%;max-width:240px;height:auto;aspect-ratio:1/1}.preview-img{width:100%}}"
}

pub(crate) async fn apply_changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let count = session.pending_writes.len();
    let mut paths = String::new();
    let mut wrote = 0;

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
        if wrote > 0 {
            paths.push_str(", ");
        }
        paths.push_str(&path.display().to_string());
        wrote += 1;
    }

    audit_log(
        &state.root_dir,
        &session.username,
        session.uid,
        "apply_changes",
        &format!("flushed {} pending writes", count),
    );

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
