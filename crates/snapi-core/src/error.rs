use std::path::PathBuf;

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum SnapiError {
    #[error("config not found: {0}")]
    ConfigNotFound(PathBuf),

    #[error("config parse error")]
    ConfigParse(#[source] anyhow::Error),

    #[error("spec not found: {0}")]
    SpecNotFound(PathBuf),

    #[error("spec parse error")]
    SpecParse(#[source] anyhow::Error),

    #[error("unresolvable $ref: {ref_path}")]
    UnresolvableRef { ref_path: String },

    #[error("circular ref is not a named schema")]
    CircularRefNotNamed,

    #[error("unsupported construct: {what}")]
    UnsupportedConstruct { what: String },

    #[error("unknown generator: {language}/{variant}")]
    UnknownGenerator { language: String, variant: String },

    #[error("generator failed for {language}")]
    GeneratorFailed {
        language: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("write failed: {path}")]
    OutputWriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
