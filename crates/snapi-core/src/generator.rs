use crate::file_tree::FileTree;
use crate::ir::api::IrApi;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PublishConfig {
    pub registry: String,
    pub access: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CollisionStrategy {
    #[default]
    Fail,
    Suffix,
}

#[derive(Debug, Clone)]
pub struct TargetConfig {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub dir: PathBuf,
    pub publish: Option<PublishConfig>,
    pub on_collision: CollisionStrategy,
}

pub trait Generator: Send + Sync {
    fn language(&self) -> &'static str;
    fn variant(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn generate(&self, api: &IrApi, config: &TargetConfig) -> anyhow::Result<FileTree>;
}
