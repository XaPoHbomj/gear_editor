use crate::{
    app_state::AppState,
    auth::{get_session, is_admin, redirect_to_login},
    i18n::{locale_from_headers, t},
    utils::audit_log,
};
use axum::{
    extract::{Multipart, OriginalUri, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct DeleteForm {
    filename: String,
}

#[derive(Deserialize)]
pub(crate) struct UpdateHadalZoneForm {
    server: u32,
    hadal_id: String,
    new_zone: u32,
}

pub(crate) async fn admin_update_hadal_zone(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    axum::extract::Form(payload): axum::extract::Form<UpdateHadalZoneForm>,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0).into_response();
    };

    if !is_admin(&session) {
        return (StatusCode::FORBIDDEN, Html("Forbidden")).into_response();
    }

    if !(1..=3).contains(&payload.server) {
        return Html("Invalid server number (1-3)").into_response();
    }

    let valid_ids = ["hadal_zone_scheduled", "hadal_zone_stable", "hadal_zone_defensive", "boss_challenge_normal", "boss_challenge_hard"];
    if !valid_ids.contains(&payload.hadal_id.as_str()) {
        return Html("Invalid hadal_id").into_response();
    }

    let script = state.root_dir.join("scripts/update_hadal_zone.sh");
    let output = std::process::Command::new("bash")
        .arg(script.to_str().unwrap_or("scripts/update_hadal_zone.sh"))
        .arg(payload.server.to_string())
        .arg(&payload.hadal_id)
        .arg(payload.new_zone.to_string())
        .output();

    match output {
        Ok(out) if out.status.success() => {
            audit_log(&state.root_dir, &session.username, session.uid, "update_hadal_zone", &format!("server={} {} -> {}", payload.server, payload.hadal_id, payload.new_zone));
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Html(format!("Script failed: {}", stderr)).into_response();
        }
        Err(e) => {
            return Html(format!("Failed to run script: {}", e)).into_response();
        }
    }

    let locale = locale_from_headers(&headers);
    Redirect::to(&format!("/dashboard?tab=status")).into_response()
}

const MIN_FREE_SPACE: u64 = 1024 * 1024 * 1024;

pub(crate) async fn admin_upload_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0).into_response();
    };
    let locale = locale_from_headers(&headers);

    if !is_admin(&session) {
        return (StatusCode::FORBIDDEN, Html("Forbidden")).into_response();
    }

    let update_dir = state.root_dir.join("client_updates/Beta/Update");
    std::fs::create_dir_all(&update_dir).ok();

    match fs2::available_space(&update_dir) {
        Ok(available) if available < MIN_FREE_SPACE => {
            return Html(format!(
                "{}: {:.2} GB {} < 1 GB",
                t(locale, "updates.upload_error_space"),
                available as f64 / (1024.0 * 1024.0 * 1024.0),
                t(locale, "updates.upload_error_space_available"),
            ))
            .into_response();
        }
        Ok(_) => {}
        Err(e) => {
            return Html(format!("Failed to check disk space: {}", e)).into_response();
        }
    }

    let mut saved = false;
    let mut saved_name = String::new();
    while let Ok(Some(field)) = multipart.next_field().await {
        let Some(file_name) = field.file_name().map(|s| s.to_string()) else {
            continue;
        };
        if !file_name.ends_with(".zip") {
            continue;
        }
        let dest = update_dir.join(&file_name);
        if dest.exists() {
            return Html(format!(
                "{} '{}' {}",
                t(locale, "updates.upload_error_exists_prefix"),
                file_name,
                t(locale, "updates.upload_error_exists_suffix"),
            ))
            .into_response();
        }

        let mut file = match tokio::fs::File::create(&dest).await {
            Ok(f) => f,
            Err(e) => {
                return Html(format!("Failed to create file: {}", e)).into_response();
            }
        };

        use futures_util::StreamExt;

        let mut stream = field;
        while let Some(bytes) = stream.next().await {
            match bytes {
                Ok(chunk) => {
                    if let Err(e) =
                        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await
                    {
                        let _ = tokio::fs::remove_file(&dest).await;
                        return Html(format!("Failed to write file: {}", e)).into_response();
                    }
                }
                Err(e) => {
                    let _ = tokio::fs::remove_file(&dest).await;
                    return Html(format!("Failed to read upload stream: {}", e)).into_response();
                }
            }
        }

        if let Err(e) = file.sync_all().await {
            let _ = tokio::fs::remove_file(&dest).await;
            return Html(format!("Failed to sync file: {}", e)).into_response();
        }

        saved_name = file_name;
        saved = true;
        break;
    }

    if !saved {
        return Html(t(locale, "updates.upload_no_file")).into_response();
    }

    audit_log(&state.root_dir, &session.username, session.uid, "upload_update", &format!("uploaded {}", saved_name));

    Redirect::to("/dashboard?tab=updates").into_response()
}

pub(crate) async fn admin_delete_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    axum::extract::Form(payload): axum::extract::Form<DeleteForm>,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0).into_response();
    };

    if !is_admin(&session) {
        return (StatusCode::FORBIDDEN, Html("Forbidden")).into_response();
    }

    let dest = state
        .root_dir
        .join("client_updates/Beta/Update")
        .join(&payload.filename);

    if !dest.exists() || !dest.starts_with(state.root_dir.join("client_updates")) {
        return (StatusCode::NOT_FOUND, Html("File not found")).into_response();
    }

    if let Err(e) = tokio::fs::remove_file(&dest).await {
        return Html(format!("Failed to delete file: {}", e)).into_response();
    }

    Redirect::to("/dashboard?tab=updates").into_response()
}
