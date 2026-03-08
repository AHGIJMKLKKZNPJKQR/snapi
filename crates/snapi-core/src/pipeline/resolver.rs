use oas3::OpenApiV3Spec;

/// A resolved spec passed through to the normalizer.
/// Cycle detection uses a stack-local visiting set in the normalizer — not here.
pub struct ResolvedSpec {
    pub inner: OpenApiV3Spec,
}

pub fn resolve(spec: OpenApiV3Spec) -> anyhow::Result<ResolvedSpec> {
    Ok(ResolvedSpec { inner: spec })
}
