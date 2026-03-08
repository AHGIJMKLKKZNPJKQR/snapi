use crate::error::SnapiError;
use indexmap::IndexMap;
use std::path::{Path, PathBuf};

pub enum FileContent {
    Text(String),
    Template {
        name: String,
        context: tera::Context,
    },
}

#[derive(Default)]
pub struct FileTree {
    pub files: IndexMap<PathBuf, FileContent>,
}

impl FileTree {
    pub fn insert(&mut self, path: impl Into<PathBuf>, content: FileContent) {
        self.files.insert(path.into(), content);
    }

    pub fn write_to_disk(&self, base: &Path, tera: &tera::Tera) -> Result<(), SnapiError> {
        for (rel_path, content) in &self.files {
            let full_path = base.join(rel_path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| SnapiError::OutputWriteFailed {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
            let text = match content {
                FileContent::Text(s) => s.clone(),
                FileContent::Template { name, context } => {
                    tera.render(name, context)
                        .map_err(|e| SnapiError::OutputWriteFailed {
                            path: full_path.clone(),
                            source: std::io::Error::other(e.to_string()),
                        })?
                }
            };
            std::fs::write(&full_path, text).map_err(|e| SnapiError::OutputWriteFailed {
                path: full_path.clone(),
                source: e,
            })?;
        }
        Ok(())
    }
}
