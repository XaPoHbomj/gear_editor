use crate::{
    app_state::AppState,
    auth::{AuthContext, csrf_input, html_escape_text},
    ctl::{self},
    i18n::{Locale, t},
    remielle_save::{self, MAX_SAVE_FILE_BYTES},
    utils::{audit_log, shared_page_css},
};
use axum::{
    extract::Multipart,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use std::{fs, io::Write, path::Path};

const MIN_FREE_SPACE: u64 = 64 * 1024 * 1024;

pub(crate) fn render_save_upload_panel(
    state: &AppState,
    player_uid: Option<u32>,
    locale: Locale,
    csrf: &str,
    server_label: &str,
    player_online: bool,
    saved: bool,
) -> String {
    let title = t(locale, "saves.title");
    let description = t(locale, "saves.description");
    let note = t(locale, "saves.note");
    let success = if saved {
        format!(
            r#"<div style="padding:10px 12px; border-radius:8px; background:#123021; border:1px solid #1f6f43; color:#b8f5cf; font-size:13px; margin-bottom:12px;">{}</div>"#,
            t(locale, "saves.success")
        )
    } else {
        String::new()
    };

    let Some(uid) = player_uid else {
        return format!(
            r#"<div class="card" style="background:#1b1f2a; padding:16px; border-radius:12px; border:1px solid #232a38;">
                <h3 style="margin-top:0;">{title}</h3>
                <p class="meta" style="margin-bottom:0;">{}</p>
            </div>"#,
            t(locale, "saves.login_required"),
            title = title,
        );
    };

    let save_path = state.state_dir.join(format!("USD_{uid}.bin"));
    let current = match fs::metadata(&save_path) {
        Ok(metadata) => {
            let updated = metadata
                .modified()
                .ok()
                .map(format_system_time)
                .unwrap_or_else(|| t(locale, "updates.unknown").to_string());
            format!(
                r#"<div style="font-weight:700;">USD_{uid}.bin</div>
                <div class="meta">{} &middot; {}</div>"#,
                format_file_size(metadata.len()),
                html_escape_text(&updated),
            )
        }
        Err(_) => format!(r#"<div class="meta">{}</div>"#, t(locale, "saves.none")),
    };

    let body = if player_online {
        format!(
            r#"<div style="padding:12px; border-radius:8px; background:#3a2212; border:1px solid #8a4b1f; color:#ffd9b8; font-size:13px;">{}</div>"#,
            t(locale, "saves.online")
        )
    } else {
        format!(
            r#"<form method="post" action="/save/upload" enctype="multipart/form-data" style="display:grid; gap:12px; margin-top:4px;">
                {csrf}
                <div>
                    <label style="display:block; margin:0 0 6px; font-size:12px; color:#9aa4b2;">{file_label}</label>
                    <input type="file" name="file" accept=".bin" required style="width:100%; box-sizing:border-box; padding:8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:13px;" />
                </div>
                <button type="submit" style="justify-self:start; padding:10px 14px; border:0; border-radius:8px; background:#4c7dff; color:#fff; font-weight:700; cursor:pointer; font-family:inherit; font-size:13px;">{upload}</button>
            </form>"#,
            csrf = csrf_input(csrf),
            file_label = html_escape_text(t(locale, "saves.file")),
            upload = t(locale, "saves.upload"),
        )
    };

    format!(
        r#"<div class="card" style="background:#1b1f2a; padding:16px; border-radius:12px; border:1px solid #232a38; display:grid; gap:14px;">
            <div>
                <h3 style="margin:0 0 6px;">{title}</h3>
                <p class="meta" style="margin:0;">{description}</p>
            </div>
            {success}
            <div style="display:grid; grid-template-columns:repeat(auto-fit, minmax(220px, 1fr)); gap:12px;">
                <div style="padding:12px; border-radius:10px; background:#121620; border:1px solid #232a38;">
                    <div class="meta">{server_label_text}</div>
                    <div style="font-weight:700;">{server_label}</div>
                </div>
                <div style="padding:12px; border-radius:10px; background:#121620; border:1px solid #232a38;">
                    <div class="meta">{target_uid_label}</div>
                    <div style="font-weight:700;">{uid}</div>
                </div>
                <div style="padding:12px; border-radius:10px; background:#121620; border:1px solid #232a38;">
                    <div class="meta">{current_label}</div>
                    {current}
                </div>
            </div>
            {body}
            <div style="padding:10px 12px; border-radius:8px; background:#0f1115; border:1px solid #1f2635; font-size:12px; color:#9aa4b2; line-height:1.5;">{note}</div>
        </div>"#,
        title = title,
        description = html_escape_text(description),
        success = success,
        server_label_text = t(locale, "saves.server"),
        server_label = html_escape_text(server_label),
        target_uid_label = t(locale, "saves.target_uid"),
        uid = uid,
        current_label = t(locale, "saves.current"),
        current = current,
        body = body,
        note = html_escape_text(note),
    )
}

fn format_file_size(bytes: u64) -> String {
    let kib = 1024.0;
    let mib = kib * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= mib {
        format!("{:.2} MB", bytes_f / mib)
    } else {
        format!("{:.1} KB", (bytes_f / kib).max(0.1))
    }
}

fn format_system_time(time: std::time::SystemTime) -> String {
    use chrono::{DateTime, Utc};
    let datetime: DateTime<Utc> = time.into();
    datetime.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn valid_save_file_name(file_name: &str) -> bool {
    if file_name.contains('/') || file_name.contains('\\') {
        return false;
    }
    let Some(rest) = file_name.strip_prefix("USD_") else {
        return false;
    };
    let Some(digits) = rest.strip_suffix(".bin") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn error_page(locale: Locale, message: &str) -> Response {
    let back = t(locale, "saves.back");
    Html(format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Gear Editor</title>
    <style>{css}</style>
</head>
<body>
    <div class="container">
        <div style="padding:14px; border-radius:10px; background:#2a1518; border:1px solid #7f1d1d; color:#fecaca; margin-bottom:14px;">{message}</div>
        <a href="/dashboard?tab=saves">{back}</a>
    </div>
</body>
</html>"#,
        lang = locale.lang_attr(),
        css = shared_page_css(),
        message = html_escape_text(message),
        back = html_escape_text(back),
    ))
    .into_response()
}

pub(crate) async fn upload_save(auth: AuthContext, mut multipart: Multipart) -> Response {
    let locale = auth.locale;

    let Some(uid) = crate::player_state::player_uid_from_map(&auth.state, auth.session.uid) else {
        return error_page(locale, t(locale, "saves.login_required"));
    };

    if ctl::player_is_online_now(&auth.state.ctl_addr, uid) {
        return error_page(locale, t(locale, "saves.online"));
    }

    let mut csrf_value: Option<String> = None;
    let mut upload: Option<(String, Vec<u8>)> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("_csrf") {
            if let Ok(value) = field.text().await {
                csrf_value = Some(value);
            }
            continue;
        }
        if field.name() != Some("file") {
            continue;
        }
        let Some(file_name) = field.file_name().map(str::to_string) else {
            continue;
        };
        let bytes = match field.bytes().await {
            Ok(bytes) => bytes,
            Err(_) => return error_page(locale, t(locale, "saves.error.too_large")),
        };
        upload = Some((file_name, bytes.to_vec()));
    }

    if !csrf_value
        .as_deref()
        .is_some_and(|value| crate::auth::csrf_ok(&auth.session_id, Some(value)))
    {
        return (StatusCode::BAD_REQUEST, Html("Invalid CSRF token")).into_response();
    }

    let Some((file_name, data)) = upload else {
        return error_page(locale, t(locale, "saves.error.no_file"));
    };

    if !valid_save_file_name(&file_name) {
        return error_page(locale, t(locale, "saves.error.filename"));
    }

    if data.len() as u64 > MAX_SAVE_FILE_BYTES {
        return error_page(locale, t(locale, "saves.error.too_large"));
    }

    if let Err(error) = remielle_save::validate_player_save(&data) {
        return error_page(locale, t(locale, error.i18n_key()));
    }

    if let Err(error) = fs::create_dir_all(&auth.state.state_dir) {
        eprintln!("upload_save: create state dir failed: {error}");
        return error_page(locale, t(locale, "saves.error.write"));
    }

    let required = (data.len() as u64)
        .saturating_mul(3)
        .saturating_add(MIN_FREE_SPACE);
    match fs2::available_space(&auth.state.state_dir) {
        Ok(available) if available < required => {
            return error_page(locale, t(locale, "saves.error.space"));
        }
        Ok(_) => {}
        Err(error) => {
            eprintln!("upload_save: disk space check failed: {error}");
            return error_page(locale, t(locale, "saves.error.write"));
        }
    }

    let dest = auth.state.state_dir.join(format!("USD_{uid}.bin"));
    let backup = auth.state.state_dir.join(format!("USD_{uid}.bin.bak"));

    if dest.exists() {
        if let Err(error) = fs::copy(&dest, &backup) {
            eprintln!("upload_save: backup failed: {error}");
            return error_page(locale, t(locale, "saves.error.backup"));
        }
        if let Ok(file) = fs::File::open(&backup) {
            let _ = file.sync_all();
        }
    }

    let temp_path = auth
        .state
        .state_dir
        .join(format!("USD_{uid}.bin.tmp.{}", rand::random::<u32>()));
    if let Err(error) = write_atomic(&temp_path, &dest, &data) {
        eprintln!("upload_save: write failed: {error}");
        let _ = fs::remove_file(&temp_path);
        return error_page(locale, t(locale, "saves.error.write"));
    }

    audit_log(
        &auth.state.root_dir,
        &auth.session.username,
        auth.session.uid,
        "upload_save",
        &format!(
            "ctl={} uid={} file={} bytes={}",
            auth.state.ctl_addr,
            uid,
            file_name,
            data.len()
        ),
    );

    Redirect::to("/dashboard?tab=saves&saved=1").into_response()
}

fn write_atomic(temp_path: &Path, dest: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(temp_path)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp_path, dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_file_name_accepts_usd_bin() {
        assert!(valid_save_file_name("USD_100.bin"));
        assert!(valid_save_file_name("USD_1.bin"));
    }

    #[test]
    fn save_file_name_rejects_other_names() {
        assert!(!valid_save_file_name("USB_100.bin"));
        assert!(!valid_save_file_name("USD_.bin"));
        assert!(!valid_save_file_name("USD_100.zip"));
        assert!(!valid_save_file_name("USD_10a.bin"));
        assert!(!valid_save_file_name("../USD_100.bin"));
        assert!(!valid_save_file_name("sub/USD_100.bin"));
        assert!(!valid_save_file_name("USD_100.bin.bak"));
    }
}
