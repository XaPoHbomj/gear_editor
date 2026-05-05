use serde::Deserialize;
use std::{
    env, fs,
    path::{Path as FsPath, PathBuf},
};

#[derive(Deserialize)]
pub(crate) struct SdkConfig {
    pub(crate) db_file: String,
}

pub(crate) fn resolve_sdk_config_path() -> PathBuf {
    if let Ok(path) = env::var("HOYO_SDK_CONFIG") {
        return PathBuf::from(path);
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cfg = root.join("hoyo-sdk/sdk_server.toml");
    if cfg.exists() {
        return cfg;
    }

    root.join("hoyo-sdk/sdk.default.toml")
}

pub(crate) fn load_sdk_config(path: &FsPath) -> SdkConfig {
    let data = fs::read_to_string(path).expect("Failed to read SDK config");
    toml::from_str(&data).expect("Invalid SDK config")
}

pub(crate) fn resolve_db_path(config_path: &FsPath, db_file: &str) -> PathBuf {
    let db_path = PathBuf::from(db_file);
    if db_path.is_absolute() {
        return db_path;
    }

    config_path
        .parent()
        .unwrap_or_else(|| FsPath::new("."))
        .join(db_path)
}
