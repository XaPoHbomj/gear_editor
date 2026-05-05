use crate::{
    AppState,
    app_state::{ServerMode, parse_server_mode},
    auth::{
        get_session, html_escape_attr, insert_session, redirect_to_login, sanitize_next_path,
        set_session, validate_login,
    },
};
use axum::{
    extract::{Form, OriginalUri, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct LoginForm {
    username: String,
    password: String,
    next: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct LoginQuery {
    next: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SwitchServerQuery {
    target: Option<String>,
    next: Option<String>,
}

pub(crate) async fn login_page(Query(query): Query<LoginQuery>) -> Html<String> {
    let next = query
        .next
        .as_deref()
        .and_then(sanitize_next_path)
        .unwrap_or_else(|| "/dashboard".to_string());
    let next_attr = html_escape_attr(&next);

    let body = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Gear Editor - Login</title>
  <style>
        body { font-family: system-ui, sans-serif; background: #0f1115; color: #e6e6e6; display: grid; place-items: center; height: 100vh; margin: 0; }
        form { background: #1b1f2a; padding: 24px; border-radius: 12px; width: 320px; box-sizing: border-box; box-shadow: 0 10px 30px rgba(0,0,0,.4); display: flex; flex-direction: column; gap: 12px; }
    h1 { font-size: 18px; margin: 0; }
    .field { display: flex; flex-direction: column; gap: 6px; }
    label { display: block; margin: 0; font-size: 12px; color: #9aa4b2; }
    input { width: 100%; box-sizing: border-box; padding: 10px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }
    button { width: 100%; padding: 10px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }
        @media (max-width: 768px) {
            body { display: flex; align-items: center; justify-content: center; height: auto; min-height: 100vh; padding: 16px; box-sizing: border-box; }
            form { width: 100%; max-width: 420px; margin: 0; box-sizing: border-box; }
        }
  </style>
</head>
<body>
  <form method="post" action="/login">
    <h1>Gear Editor</h1>
        <input type="hidden" name="next" value="{next_attr}" />
        <div class="field">
            <label for="username">Username</label>
            <input id="username" name="username" autocomplete="username" required />
        </div>
        <div class="field">
            <label for="password">Password</label>
            <input id="password" name="password" type="password" autocomplete="current-password" required />
        </div>
    <button type="submit">Sign in</button>
  </form>
</body>
</html>"#
    .replace("{next_attr}", &next_attr);

    Html(body)
}

pub(crate) async fn login(
    State(state): State<AppState>,
    Form(payload): Form<LoginForm>,
) -> impl IntoResponse {
    let response: Response =
        match validate_login(&state.db_path, &payload.username, &payload.password) {
            Ok(Some(session)) => {
                let session_id = insert_session(session);

                let mut headers = HeaderMap::new();
                headers.insert(
                    header::SET_COOKIE,
                    format!("ge_session={}; HttpOnly; SameSite=Lax; Path=/", session_id)
                        .parse()
                        .unwrap(),
                );

                let next = payload
                    .next
                    .as_deref()
                    .and_then(sanitize_next_path)
                    .unwrap_or_else(|| "/dashboard".to_string());
                (headers, Redirect::to(&next)).into_response()
            }
            Ok(None) => (StatusCode::UNAUTHORIZED, Html("Invalid credentials")).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Html("Login failed")).into_response(),
        };

    response
}

pub(crate) async fn switch_server(
    headers: HeaderMap,
    original_uri: OriginalUri,
    Query(query): Query<SwitchServerQuery>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let mode = parse_server_mode(query.target.as_deref().unwrap_or("beta"));
    let next = query
        .next
        .as_deref()
        .and_then(sanitize_next_path)
        .unwrap_or_else(|| "/dashboard".to_string());

    // Pending writes are path-bound; clear them on server switch to avoid cross-server apply.
    session.pending_writes.clear();
    set_session(session_id, session);

    let mut response = Redirect::to(&next).into_response();
    let value = match mode {
        ServerMode::Beta => "gear_server=beta; Path=/; SameSite=Lax",
        ServerMode::Prod => "gear_server=prod; Path=/; SameSite=Lax",
    };
    if let Ok(header_value) = HeaderValue::from_str(value) {
        response
            .headers_mut()
            .insert(header::SET_COOKIE, header_value);
    }

    response
}
