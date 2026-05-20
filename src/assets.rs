use crate::app_state::AppState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use std::path::Path as FsPath;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
};
use tokio_util::io::ReaderStream;

pub(crate) async fn asset_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let rel_path = FsPath::new(&path);
    if rel_path.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let full_path = state.root_dir.join(rel_path);
    let file = match File::open(&full_path).await {
        Ok(file) => file,
        Err(err) => {
            return if err.kind() == std::io::ErrorKind::NotFound {
                StatusCode::NOT_FOUND.into_response()
            } else {
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            };
        }
    };

    let file_len = match file.metadata().await {
        Ok(metadata) => metadata.len(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let content_type = content_type_for_path(&full_path);
    let cache_control = cache_control_for_path(&full_path);
    let range_header = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    if let Some(range_value) = range_header {
        let Some((start, end)) = parse_http_range(&range_value, file_len) else {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes */{file_len}"))
                .body(Body::empty())
                .unwrap();
        };

        let mut ranged_file = file;
        if ranged_file.seek(SeekFrom::Start(start)).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        let chunk_len = end - start + 1;
        let stream = ReaderStream::new(ranged_file.take(chunk_len));

        return Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, cache_control)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(
                header::CONTENT_RANGE,
                format!("bytes {start}-{end}/{file_len}"),
            )
            .header(header::CONTENT_LENGTH, chunk_len.to_string())
            .body(Body::from_stream(stream))
            .unwrap();
    }

    let stream = ReaderStream::new(file);
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, cache_control)
        .header(header::ACCEPT_RANGES, "bytes");

    builder = builder.header(header::CONTENT_LENGTH, file_len.to_string());

    builder.body(Body::from_stream(stream)).unwrap()
}

fn parse_http_range(value: &str, file_len: u64) -> Option<(u64, u64)> {
    if file_len == 0 {
        return None;
    }

    let bytes_prefix = "bytes=";
    if !value.starts_with(bytes_prefix) {
        return None;
    }

    // We intentionally support only a single byte range.
    let first_range = value[bytes_prefix.len()..].split(',').next()?.trim();
    let (start_raw, end_raw) = first_range.split_once('-')?;

    if start_raw.is_empty() {
        let suffix_len = end_raw.parse::<u64>().ok()?;
        if suffix_len == 0 {
            return None;
        }
        let start = file_len.saturating_sub(suffix_len);
        let end = file_len - 1;
        return Some((start, end));
    }

    let start = start_raw.parse::<u64>().ok()?;
    if start >= file_len {
        return None;
    }

    let end = if end_raw.is_empty() {
        file_len - 1
    } else {
        let parsed_end = end_raw.parse::<u64>().ok()?;
        parsed_end.min(file_len - 1)
    };

    if end < start {
        return None;
    }

    Some((start, end))
}

fn content_type_for_path(path: &FsPath) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "zip" => "application/zip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn cache_control_for_path(path: &FsPath) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" => "public, max-age=604800, immutable",
        _ => "no-cache",
    }
}
