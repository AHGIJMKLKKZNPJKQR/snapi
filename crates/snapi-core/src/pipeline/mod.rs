pub mod normalizer;
pub mod parser;
pub mod resolver;

use crate::ir::api::IrApi;
use std::path::Path;

pub fn load(spec_path: &Path) -> anyhow::Result<IrApi> {
    let raw = parser::parse(spec_path)?;
    let resolved = resolver::resolve(raw)?;
    normalizer::normalize(resolved)
}
