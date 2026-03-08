use crate::config::{Config, InputConfig, PackageConfig, RawTargetConfig};
use dialoguer::{Confirm, Input, MultiSelect};
use miette::IntoDiagnostic;
use std::path::{Path, PathBuf};

pub(crate) struct TargetArgs {
    pub language: String,
    pub variant: String,
    pub dir: String,
    pub name: Option<String>,
}

pub(crate) fn write_config(
    config_path: &Path,
    spec: &str,
    version: &str,
    targets: Vec<TargetArgs>,
) -> miette::Result<()> {
    let raw_targets = targets
        .into_iter()
        .map(|t| RawTargetConfig {
            language: t.language,
            variant: Some(t.variant),
            dir: PathBuf::from(t.dir),
            name: t.name,
            description: None,
            version: None,
            publish: None,
        })
        .collect();

    let cfg = Config {
        input: InputConfig {
            spec: PathBuf::from(spec),
        },
        package: Some(PackageConfig {
            version: Some(version.to_string()),
        }),
        target: raw_targets,
    };

    let toml_str = toml::to_string_pretty(&cfg).into_diagnostic()?;
    std::fs::write(config_path, toml_str).into_diagnostic()?;
    Ok(())
}

pub fn run(config_path: &Path) -> miette::Result<()> {
    if config_path.exists() {
        let overwrite = Confirm::new()
            .with_prompt(format!(
                "{} already exists. Overwrite?",
                config_path.display()
            ))
            .default(false)
            .interact()
            .into_diagnostic()?;
        if !overwrite {
            return Ok(());
        }
    }

    let spec: String = Input::new()
        .with_prompt("Path to OpenAPI spec file")
        .default("openapi.yaml".to_string())
        .interact_text()
        .into_diagnostic()?;

    let version: String = Input::new()
        .with_prompt("Default package version")
        .default("0.1.0".to_string())
        .interact_text()
        .into_diagnostic()?;

    let generators = snapi_generators::registry();
    let labels: Vec<String> = generators
        .iter()
        .map(|g| {
            format!(
                "{}/{} \u{2014} {}",
                g.language(),
                g.variant(),
                g.description()
            )
        })
        .collect();

    let selections = loop {
        let sel = MultiSelect::new()
            .with_prompt("Select generation targets (space to toggle, enter to confirm)")
            .items(&labels)
            .interact()
            .into_diagnostic()?;
        if sel.is_empty() {
            eprintln!("Please select at least one target.");
        } else {
            break sel;
        }
    };

    let mut target_args: Vec<TargetArgs> = Vec::new();
    for idx in selections {
        let gen = &generators[idx];
        let default_dir = format!("./sdks/{}", gen.language());

        let dir: String = Input::new()
            .with_prompt(format!(
                "Output directory for {}/{}",
                gen.language(),
                gen.variant()
            ))
            .default(default_dir)
            .interact_text()
            .into_diagnostic()?;

        let name_input: String = Input::new()
            .with_prompt("Package name (leave empty to auto-detect from spec)")
            .allow_empty(true)
            .interact_text()
            .into_diagnostic()?;

        let name = if name_input.trim().is_empty() {
            None
        } else {
            Some(name_input.trim().to_string())
        };

        target_args.push(TargetArgs {
            language: gen.language().to_string(),
            variant: gen.variant().to_string(),
            dir,
            name,
        });
    }

    write_config(config_path, &spec, &version, target_args)?;

    println!(
        "Created {} \u{2014} run `snapi generate` to build your SDK",
        config_path.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_config;

    fn single_target(dir: &str, name: Option<&str>) -> Vec<TargetArgs> {
        vec![TargetArgs {
            language: "typescript".to_string(),
            variant: "fetch".to_string(),
            dir: dir.to_string(),
            name: name.map(str::to_string),
        }]
    }

    #[test]
    fn write_config_creates_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        // Remove so write_config creates it fresh
        std::fs::remove_file(&path).unwrap();

        write_config(
            &path,
            "openapi.yaml",
            "0.1.0",
            single_target("./sdks/ts", None),
        )
        .unwrap();

        assert!(path.exists());
    }

    #[test]
    fn written_toml_round_trips_through_load_config() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_config(
            tmp.path(),
            "openapi.yaml",
            "0.1.0",
            single_target("./sdks/ts", None),
        )
        .unwrap();

        load_config(tmp.path()).expect("load_config must parse the written file");
    }

    #[test]
    fn write_config_spec_path_is_preserved() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_config(
            tmp.path(),
            "specs/my-api.yaml",
            "1.2.3",
            single_target("./out", None),
        )
        .unwrap();

        let cfg = load_config(tmp.path()).unwrap();
        assert_eq!(cfg.input.spec, PathBuf::from("specs/my-api.yaml"));
    }

    #[test]
    fn write_config_package_version_is_preserved() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_config(
            tmp.path(),
            "openapi.yaml",
            "3.0.0",
            single_target("./out", None),
        )
        .unwrap();

        let cfg = load_config(tmp.path()).unwrap();
        assert_eq!(
            cfg.package.as_ref().and_then(|p| p.version.as_deref()),
            Some("3.0.0")
        );
    }

    #[test]
    fn write_config_target_language_and_variant() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_config(
            tmp.path(),
            "openapi.yaml",
            "0.1.0",
            single_target("./sdks/ts", None),
        )
        .unwrap();

        let cfg = load_config(tmp.path()).unwrap();
        assert_eq!(cfg.target.len(), 1);
        assert_eq!(cfg.target[0].language, "typescript");
        assert_eq!(cfg.target[0].variant.as_deref(), Some("fetch"));
    }

    #[test]
    fn write_config_target_dir_is_preserved() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_config(
            tmp.path(),
            "openapi.yaml",
            "0.1.0",
            single_target("./sdks/my-ts-sdk", None),
        )
        .unwrap();

        let cfg = load_config(tmp.path()).unwrap();
        assert_eq!(cfg.target[0].dir, PathBuf::from("./sdks/my-ts-sdk"));
    }

    #[test]
    fn write_config_name_none_when_empty() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_config(
            tmp.path(),
            "openapi.yaml",
            "0.1.0",
            single_target("./sdks/ts", None),
        )
        .unwrap();

        let cfg = load_config(tmp.path()).unwrap();
        assert!(cfg.target[0].name.is_none());
    }

    #[test]
    fn write_config_name_some_when_provided() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_config(
            tmp.path(),
            "openapi.yaml",
            "0.1.0",
            single_target("./sdks/ts", Some("my-sdk")),
        )
        .unwrap();

        let cfg = load_config(tmp.path()).unwrap();
        assert_eq!(cfg.target[0].name.as_deref(), Some("my-sdk"));
    }

    #[test]
    fn write_config_multiple_targets() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let targets = vec![
            TargetArgs {
                language: "typescript".to_string(),
                variant: "fetch".to_string(),
                dir: "./sdks/ts".to_string(),
                name: None,
            },
            TargetArgs {
                language: "typescript".to_string(),
                variant: "fetch".to_string(),
                dir: "./sdks/ts2".to_string(),
                name: Some("alt-sdk".to_string()),
            },
        ];
        write_config(tmp.path(), "openapi.yaml", "0.1.0", targets).unwrap();

        let cfg = load_config(tmp.path()).unwrap();
        assert_eq!(cfg.target.len(), 2);
        assert_eq!(cfg.target[1].name.as_deref(), Some("alt-sdk"));
        assert_eq!(cfg.target[1].dir, PathBuf::from("./sdks/ts2"));
    }

    #[test]
    fn write_config_overwrites_existing_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();

        // Write once with version 1.0.0
        write_config(
            tmp.path(),
            "old.yaml",
            "1.0.0",
            single_target("./sdks/ts", None),
        )
        .unwrap();

        // Overwrite with version 2.0.0
        write_config(
            tmp.path(),
            "new.yaml",
            "2.0.0",
            single_target("./sdks/ts", None),
        )
        .unwrap();

        let cfg = load_config(tmp.path()).unwrap();
        assert_eq!(cfg.input.spec, PathBuf::from("new.yaml"));
        assert_eq!(
            cfg.package.as_ref().and_then(|p| p.version.as_deref()),
            Some("2.0.0")
        );
    }

    #[test]
    fn written_toml_contains_expected_keys() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_config(
            tmp.path(),
            "openapi.yaml",
            "0.1.0",
            single_target("./sdks/ts", Some("my-sdk")),
        )
        .unwrap();

        let content = std::fs::read_to_string(tmp.path()).unwrap();
        assert!(content.contains("[input]"), "missing [input] section");
        assert!(content.contains("[package]"), "missing [package] section");
        assert!(content.contains("[[target]]"), "missing [[target]] array");
        assert!(content.contains("spec ="), "missing spec key");
        assert!(content.contains("language ="), "missing language key");
    }
}
