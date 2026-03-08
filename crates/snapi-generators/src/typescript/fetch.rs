use super::{render_models, render_resources};
use snapi_core::file_tree::{FileContent, FileTree};
use snapi_core::generator::{Generator, TargetConfig};
use snapi_core::ir::api::IrApi;
use snapi_core::utils::case::to_pascal_case;

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
        let models_content = render_models(&api.schemas);
        tree.insert("src/models.ts", FileContent::Text(models_content));

        // src/resources/<tag>.ts
        let resources = render_resources(&api.operations);
        for (tag, code) in &resources {
            tree.insert(
                format!("src/resources/{}.ts", tag.to_lowercase().replace(' ', "_")),
                FileContent::Text(code.clone()),
            );
        }

        // src/client.ts
        let client_code = render_client(api, &resources);
        tree.insert("src/client.ts", FileContent::Text(client_code));

        // src/index.ts
        let mut index_code = String::new();
        index_code.push_str("export * from \"./client\";\n");
        index_code.push_str("export * from \"./models\";\n");
        for (tag, _) in &resources {
            index_code.push_str(&format!(
                "export * from \"./resources/{}\";\n",
                tag.to_lowercase().replace(' ', "_")
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

fn render_client(api: &IrApi, resources: &[(String, String)]) -> String {
    let mut code = String::new();

    // Imports
    for (tag, _) in resources {
        let class_name = format!("{}Resource", to_pascal_case(tag));
        let file_name = tag.to_lowercase().replace(' ', "_");
        code.push_str(&format!(
            "import {{ {} }} from \"./resources/{}\";\n",
            class_name, file_name
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
    for (tag, _) in resources {
        let class_name = format!("{}Resource", to_pascal_case(tag));
        let field_name = snapi_core::utils::case::to_camel_case(tag);
        code.push_str(&format!("  readonly {}: {};\n", field_name, class_name));
    }
    code.push('\n');

    // Constructor
    code.push_str("  constructor(private options: ApiClientOptions) {\n");
    code.push_str("    const headers: Record<string, string> = { ...options.headers };\n");
    code.push_str(
        "    if (options.apiKey) headers[\"Authorization\"] = `Bearer ${options.apiKey}`;\n",
    );
    for (tag, _) in resources {
        let class_name = format!("{}Resource", to_pascal_case(tag));
        let field_name = snapi_core::utils::case::to_camel_case(tag);
        code.push_str(&format!(
            "    this.{} = new {}(options.baseUrl, headers);\n",
            field_name, class_name
        ));
    }
    code.push_str("  }\n");
    code.push_str("}\n");

    code
}
