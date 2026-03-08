use crate::error::SnapiError;
use anyhow::Context;
use std::path::Path;

pub fn parse(spec_path: &Path) -> anyhow::Result<oas3::OpenApiV3Spec> {
    if !spec_path.exists() {
        return Err(SnapiError::SpecNotFound(spec_path.to_path_buf()).into());
    }

    let content = std::fs::read_to_string(spec_path)
        .with_context(|| format!("failed to read {}", spec_path.display()))?;

    let ext = spec_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let spec: oas3::OpenApiV3Spec = match ext {
        "json" => serde_json::from_str(&content).map_err(|e| SnapiError::SpecParse(e.into()))?,
        _ => serde_yaml::from_str(&content).map_err(|e| SnapiError::SpecParse(e.into()))?,
    };

    Ok(spec)
}
