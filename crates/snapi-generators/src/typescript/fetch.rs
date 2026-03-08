use super::{render_models, render_resources, RenderedResource};
use snapi_core::file_tree::{FileContent, FileTree};
use snapi_core::generator::{Generator, TargetConfig};
use snapi_core::ir::api::IrApi;

pub struct FetchGenerator;

impl Generator for FetchGenerator {
    fn language(&self) -> &'static str {
        "typescript"
    }
    fn variant(&self) -> &'static str {
        "fetch"
    }
    fn description(&self) -> &'static str {
        "TypeScript SDK using the native fetch API"
    }

    fn generate(&self, api: &IrApi, config: &TargetConfig) -> anyhow::Result<FileTree> {
        let mut tree = FileTree::default();

        // src/models.ts
        let models_content = render_models(&api.schemas, config.on_collision)?;
        tree.insert("src/models.ts", FileContent::Text(models_content));

        // src/resources/<tag>.ts
        let resources = render_resources(&api.operations, config.on_collision)?;
        for res in &resources {
            tree.insert(
                format!(
                    "src/resources/{}.ts",
                    res.tag.to_lowercase().replace(' ', "_")
                ),
                FileContent::Text(res.code.clone()),
            );
        }

        // src/client.ts
        let client_code = render_client(api, &resources);
        tree.insert("src/client.ts", FileContent::Text(client_code));

        // src/index.ts
        let mut index_code = String::new();
        index_code.push_str("export * from \"./client\";\n");
        index_code.push_str("export * from \"./models\";\n");
        for res in &resources {
            index_code.push_str(&format!(
                "export * from \"./resources/{}\";\n",
                res.tag.to_lowercase().replace(' ', "_")
            ));
        }
        tree.insert("src/index.ts", FileContent::Text(index_code));

        // package.json via Tera template
        let mut ctx = tera::Context::new();
        ctx.insert("name", &config.name);
        ctx.insert("version", &config.version);
        ctx.insert(
            "description",
            &config.description.clone().unwrap_or_default(),
        );
        tree.insert(
            "package.json",
            FileContent::Template {
                name: "typescript/package.json.tera".to_string(),
                context: ctx,
            },
        );

        // tsconfig.json via Tera template
        let ctx2 = tera::Context::new();
        tree.insert(
            "tsconfig.json",
            FileContent::Template {
                name: "typescript/tsconfig.json.tera".to_string(),
                context: ctx2,
            },
        );

        Ok(tree)
    }
}

fn render_client(api: &IrApi, resources: &[RenderedResource]) -> String {
    let mut code = String::new();

    // Imports
    for res in resources {
        let file_name = res.tag.to_lowercase().replace(' ', "_");
        code.push_str(&format!(
            "import {{ {} }} from \"./resources/{}\";\n",
            res.class_name, file_name
        ));
    }
    code.push('\n');

    code.push_str("export interface ApiClientOptions {\n");
    code.push_str("  baseUrl: string;\n");
    code.push_str("  apiKey?: string;\n");
    code.push_str("  headers?: Record<string, string>;\n");
    code.push_str("}\n\n");

    code.push_str(&format!("/** {} */\n", api.title));
    code.push_str("export class ApiClient {\n");

    // Resource fields
    for res in resources {
        let field_name = snapi_core::utils::case::to_camel_case(&res.tag);
        code.push_str(&format!("  readonly {}: {};\n", field_name, res.class_name));
    }
    code.push('\n');

    // Constructor
    code.push_str("  constructor(private options: ApiClientOptions) {\n");
    code.push_str("    const headers: Record<string, string> = { ...options.headers };\n");
    code.push_str(
        "    if (options.apiKey) headers[\"Authorization\"] = `Bearer ${options.apiKey}`;\n",
    );
    for res in resources {
        let field_name = snapi_core::utils::case::to_camel_case(&res.tag);
        code.push_str(&format!(
            "    this.{} = new {}(options.baseUrl, headers);\n",
            field_name, res.class_name
        ));
    }
    code.push_str("  }\n");
    code.push_str("}\n");

    code
}
