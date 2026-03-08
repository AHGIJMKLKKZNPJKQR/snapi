#![allow(dead_code)]
use serde::Deserialize;
use snapi_core::error::SnapiError;
use snapi_core::generator::PublishConfig;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct InputConfig {
    pub spec: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct PackageConfig {
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RawTargetConfig {
    pub language: String,
    pub variant: Option<String>,
    pub dir: PathBuf,
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub publish: Option<PublishConfig>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub input: InputConfig,
    pub package: Option<PackageConfig>,
    pub target: Vec<RawTargetConfig>,
}

pub fn load_config(path: &Path) -> Result<Config, SnapiError> {
    if !path.exists() {
        return Err(SnapiError::ConfigNotFound(path.to_path_buf()));
    }
    let content = std::fs::read_to_string(path).map_err(|e| SnapiError::ConfigParse(e.into()))?;
    toml::from_str(&content).map_err(|e| SnapiError::ConfigParse(e.into()))
}
