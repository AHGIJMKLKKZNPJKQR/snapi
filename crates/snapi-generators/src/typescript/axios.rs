use snapi_core::file_tree::FileTree;
use snapi_core::generator::{Generator, TargetConfig};
use snapi_core::ir::api::IrApi;

pub struct AxiosGenerator;

impl Generator for AxiosGenerator {
    fn language(&self) -> &'static str {
        "typescript"
    }
    fn variant(&self) -> &'static str {
        "axios"
    }
    fn description(&self) -> &'static str {
        "TypeScript SDK using axios"
    }

    fn generate(&self, api: &IrApi, config: &TargetConfig) -> anyhow::Result<FileTree> {
        // Stub: same as fetch for now
        super::fetch::FetchGenerator.generate(api, config)
    }
}
