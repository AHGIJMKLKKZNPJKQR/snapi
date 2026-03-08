#![recursion_limit = "2048"]
use clap::Parser;
use miette::IntoDiagnostic;
use std::path::PathBuf;

mod config;

#[derive(clap::Parser)]
#[command(
    name = "snapi",
    about = "Generate idiomatic SDKs from OpenAPI 3.1 specs"
)]
enum Cli {
    /// Generate SDKs for all configured targets
    Generate {
        /// Filter to a specific target language
        #[arg(long)]
        target: Option<String>,
        /// Path to snapi.toml config file
        #[arg(long, default_value = "snapi.toml")]
        config: PathBuf,
    },
    /// List all available generators
    ListGenerators,
    /// Validate the spec without generating
    Validate {
        #[arg(long, default_value = "snapi.toml")]
        config: PathBuf,
    },
    /// Dump the intermediate representation as JSON
    DumpIr {
        #[arg(long, default_value = "snapi.toml")]
        config: PathBuf,
    },
}

fn anyhow_to_miette(e: anyhow::Error) -> miette::Error {
    miette::miette!("{:#}", e)
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    match cli {
        Cli::Generate {
            target,
            config: config_path,
        } => {
            let cfg = config::load_config(&config_path).into_diagnostic()?;
            let spec_path = &cfg.input.spec;
            let ir = snapi_core::pipeline::load(spec_path).map_err(anyhow_to_miette)?;

            let generators = snapi_generators::registry();
            let tera = load_templates().map_err(anyhow_to_miette)?;

            for raw_target in &cfg.target {
                let lang = &raw_target.language;
                let variant = raw_target.variant.as_deref().unwrap_or("fetch");

                if let Some(filter) = &target {
                    if lang != filter {
                        continue;
                    }
                }

                let gen = generators
                    .iter()
                    .find(|g| g.language() == lang && g.variant() == variant)
                    .ok_or_else(|| snapi_core::error::SnapiError::UnknownGenerator {
                        language: lang.clone(),
                        variant: variant.to_string(),
                    })
                    .into_diagnostic()?;

                let target_config = snapi_core::generator::TargetConfig {
                    name: raw_target.name.clone().unwrap_or_else(|| {
                        format!("{}-sdk", ir.title.to_lowercase().replace(' ', "-"))
                    }),
                    version: raw_target
                        .version
                        .clone()
                        .or_else(|| cfg.package.as_ref().and_then(|p| p.version.clone()))
                        .unwrap_or_else(|| "0.1.0".to_string()),
                    description: raw_target.description.clone(),
                    dir: raw_target.dir.clone(),
                    publish: raw_target.publish.clone(),
                };

                let file_tree = gen
                    .generate(&ir, &target_config)
                    .map_err(|e| snapi_core::error::SnapiError::GeneratorFailed {
                        language: lang.clone(),
                        source: e,
                    })
                    .into_diagnostic()?;

                file_tree
                    .write_to_disk(&raw_target.dir, &tera)
                    .into_diagnostic()?;
                println!(
                    "Generated {} ({}) -> {}",
                    lang,
                    variant,
                    raw_target.dir.display()
                );
            }
        }

        Cli::ListGenerators => {
            let generators = snapi_generators::registry();
            println!("{:<15} {:<15} Description", "Language", "Variant");
            println!("{}", "-".repeat(60));
            for gen in &generators {
                println!(
                    "{:<15} {:<15} {}",
                    gen.language(),
                    gen.variant(),
                    gen.description()
                );
            }
        }

        Cli::Validate {
            config: config_path,
        } => {
            let cfg = config::load_config(&config_path).into_diagnostic()?;
            let spec_path = &cfg.input.spec;
            snapi_core::pipeline::load(spec_path).map_err(anyhow_to_miette)?;
            println!("OK");
        }

        Cli::DumpIr {
            config: config_path,
        } => {
            let cfg = config::load_config(&config_path).into_diagnostic()?;
            let spec_path = &cfg.input.spec;
            let ir = snapi_core::pipeline::load(spec_path).map_err(anyhow_to_miette)?;
            let json = serde_json::to_string_pretty(&ir).into_diagnostic()?;
            println!("{}", json);
        }
    }

    Ok(())
}

fn load_templates() -> anyhow::Result<tera::Tera> {
    let mut tera = tera::Tera::default();

    tera.add_raw_template(
        "typescript/package.json.tera",
        include_str!("../../snapi-generators/templates/typescript/package.json.tera"),
    )?;
    tera.add_raw_template(
        "typescript/tsconfig.json.tera",
        include_str!("../../snapi-generators/templates/typescript/tsconfig.json.tera"),
    )?;
    tera.add_raw_template(
        "typescript/README.md.tera",
        include_str!("../../snapi-generators/templates/typescript/README.md.tera"),
    )?;

    Ok(tera)
}
