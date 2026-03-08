use oas3::OpenApiV3Spec;
use std::collections::HashSet;

/// A resolved spec where schema references have been tracked
/// (oas3 handles ref resolution internally, so this is mostly a passthrough)
pub struct ResolvedSpec {
    pub inner: OpenApiV3Spec,
    pub visited_schemas: HashSet<String>,
}

pub fn resolve(spec: OpenApiV3Spec) -> anyhow::Result<ResolvedSpec> {
    let mut visited = HashSet::new();

    // Pre-populate visited schemas to detect cycles
    if let Some(components) = &spec.components {
        for name in components.schemas.keys() {
            visited.insert(name.clone());
        }
    }

    Ok(ResolvedSpec {
        inner: spec,
        visited_schemas: visited,
    })
}
