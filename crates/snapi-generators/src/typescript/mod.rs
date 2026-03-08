pub mod axios;
pub mod fetch;

use indexmap::IndexMap;
use snapi_core::ir::operation::IrOperation;
use snapi_core::ir::types::{IrObject, IrType};
use snapi_core::utils::case::{to_camel_case, to_pascal_case, to_snake_case};

pub fn render_type(ty: &IrType) -> String {
    match ty {
        IrType::String(_) => "string".to_string(),
        IrType::Integer(_) | IrType::Float(_) => "number".to_string(),
        IrType::Boolean => "boolean".to_string(),
        IrType::Null => "null".to_string(),
        IrType::Array { items, .. } => format!("{}[]", render_type(items)),
        IrType::Map(v) => format!("Record<string, {}>", render_type(v)),
        IrType::Object(o) => {
            if let Some(name) = &o.name {
                to_pascal_case(name)
            } else {
                render_inline_object(o)
            }
        }
        IrType::Optional(inner) => format!("{} | null", render_type(inner)),
        IrType::Any => "unknown".to_string(),
        IrType::Recursive(name) => to_pascal_case(name),
        IrType::Enum(e) => to_pascal_case(&e.name),
        IrType::Union(variants) => variants
            .iter()
            .map(render_type)
            .collect::<Vec<_>>()
            .join(" | "),
        IrType::Intersection(objects) => objects
            .iter()
            .map(|o| {
                if let Some(name) = &o.name {
                    to_pascal_case(name)
                } else {
                    render_inline_object(o)
                }
            })
            .collect::<Vec<_>>()
            .join(" & "),
    }
}

fn render_inline_object(obj: &IrObject) -> String {
    if obj.fields.is_empty() {
        return "Record<string, unknown>".to_string();
    }
    let fields: Vec<String> = obj
        .fields
        .iter()
        .map(|(name, field)| {
            let opt = if field.required { "" } else { "?" };
            format!("{}{}: {}", name, opt, render_type(&field.ty))
        })
        .collect();
    format!("{{ {} }}", fields.join("; "))
}

pub fn render_models(schemas: &IndexMap<String, IrType>) -> String {
    let mut output = String::new();
    for (name, ty) in schemas {
        let ts_name = to_pascal_case(name);
        match ty {
            IrType::Object(obj) => {
                output.push_str(&format!("export interface {} {{\n", ts_name));
                for (field_name, field) in &obj.fields {
                    let opt = if field.required { "" } else { "?" };
                    if let Some(desc) = &field.description {
                        output.push_str(&format!("  /** {} */\n", desc));
                    }
                    output.push_str(&format!(
                        "  {}{}: {};\n",
                        field_name,
                        opt,
                        render_type(&field.ty)
                    ));
                }
                output.push_str("}\n\n");
            }
            IrType::Enum(e) => {
                output.push_str(&format!("export type {} =\n", ts_name));
                let variants: Vec<String> = e
                    .variants
                    .iter()
                    .map(|v| format!("  | \"{}\"", to_snake_case(&v.name).to_uppercase()))
                    .collect();
                output.push_str(&variants.join("\n"));
                output.push_str(";\n\n");
            }
            _ => {
                output.push_str(&format!(
                    "export type {} = {};\n\n",
                    ts_name,
                    render_type(ty)
                ));
            }
        }
    }
    output
}

pub fn render_resources(operations: &[IrOperation]) -> Vec<(String, String)> {
    // Group by tag
    let mut by_tag: IndexMap<String, Vec<&IrOperation>> = IndexMap::new();
    for op in operations {
        let tag = op
            .tags
            .first()
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        by_tag.entry(tag).or_default().push(op);
    }

    let mut result = vec![];
    for (tag, ops) in &by_tag {
        let class_name = format!("{}Resource", to_pascal_case(tag));
        let mut code = String::new();

        // Collect imports needed
        let mut type_imports: Vec<String> = vec![];
        for op in ops.iter() {
            for r in &op.responses {
                if let Some(ty) = &r.ty {
                    collect_type_names(ty, &mut type_imports);
                }
            }
            if let Some(body) = &op.body {
                collect_type_names(&body.ty, &mut type_imports);
            }
        }
        type_imports.sort();
        type_imports.dedup();

        if !type_imports.is_empty() {
            code.push_str(&format!(
                "import type {{ {} }} from \"../models\";\n\n",
                type_imports.join(", ")
            ));
        }

        code.push_str(&format!("export class {} {{\n", class_name));
        code.push_str("  constructor(private baseUrl: string, private headers: Record<string, string> = {}) {}\n\n");

        for op in ops.iter() {
            let method_name = to_camel_case(&op.id);

            // Build params signature
            let path_params: Vec<_> = op
                .params
                .iter()
                .filter(|p| matches!(p.location, snapi_core::ir::operation::ParamLocation::Path))
                .collect();
            let query_params: Vec<_> = op
                .params
                .iter()
                .filter(|p| matches!(p.location, snapi_core::ir::operation::ParamLocation::Query))
                .collect();

            // Determine return type
            let return_type = op
                .responses
                .iter()
                .find(|r| r.status >= 200 && r.status < 300)
                .and_then(|r| r.ty.as_ref())
                .map(render_type)
                .unwrap_or_else(|| "void".to_string());

            // Build parameter list
            let mut sig_parts = vec![];
            for p in &path_params {
                sig_parts.push(format!(
                    "{}: {}",
                    to_camel_case(&p.name),
                    render_type(&p.ty)
                ));
            }
            if !query_params.is_empty() {
                let qp_fields: Vec<String> = query_params
                    .iter()
                    .map(|p| {
                        let opt = if p.required { "" } else { "?" };
                        format!("{}{}: {}", to_camel_case(&p.name), opt, render_type(&p.ty))
                    })
                    .collect();
                sig_parts.push(format!("query?: {{ {} }}", qp_fields.join("; ")));
            }
            if let Some(body) = &op.body {
                sig_parts.push(format!("body: {}", render_type(&body.ty)));
            }

            // Build URL
            let mut url = op.path.clone();
            for p in &path_params {
                url = url.replace(
                    &format!("{{{}}}", p.name),
                    &format!("${{{}}}", to_camel_case(&p.name)),
                );
            }

            let method_str = format!("{:?}", op.method).to_uppercase();
            let has_body = op.body.is_some();

            if let Some(summary) = &op.summary {
                code.push_str(&format!("  /** {} */\n", summary));
            }
            code.push_str(&format!(
                "  async {}({}): Promise<{}> {{\n",
                method_name,
                sig_parts.join(", "),
                return_type
            ));
            code.push_str(&format!(
                "    const url = new URL(`${{this.baseUrl}}{}`).toString();\n",
                url
            ));
            if !query_params.is_empty() {
                code.push_str("    if (query) {\n");
                code.push_str("      const params = new URLSearchParams();\n");
                code.push_str("      Object.entries(query).forEach(([k, v]) => v != null && params.set(k, String(v)));\n");
                code.push_str("      url + '?' + params.toString();\n");
                code.push_str("    }\n");
            }
            code.push_str("    const response = await fetch(url, {\n");
            code.push_str(&format!("      method: \"{}\",\n", method_str));
            code.push_str(
                "      headers: { \"Content-Type\": \"application/json\", ...this.headers },\n",
            );
            if has_body {
                code.push_str("      body: JSON.stringify(body),\n");
            }
            code.push_str("    });\n");
            code.push_str("    if (!response.ok) throw new Error(`HTTP ${response.status}`);\n");
            if return_type == "void" {
                code.push_str("    return;\n");
            } else {
                code.push_str("    return response.json() as Promise<");
                code.push_str(&return_type);
                code.push_str(">;\n");
            }
            code.push_str("  }\n\n");
        }
        code.push_str("}\n");
        result.push((tag.clone(), code));
    }
    result
}

fn collect_type_names(ty: &IrType, names: &mut Vec<String>) {
    match ty {
        IrType::Object(o) => {
            if let Some(name) = &o.name {
                names.push(to_pascal_case(name));
            }
        }
        IrType::Enum(e) => names.push(to_pascal_case(&e.name)),
        IrType::Recursive(name) => names.push(to_pascal_case(name)),
        IrType::Array { items, .. } => collect_type_names(items, names),
        IrType::Optional(inner) => collect_type_names(inner, names),
        IrType::Map(v) => collect_type_names(v, names),
        IrType::Union(variants) => variants.iter().for_each(|v| collect_type_names(v, names)),
        _ => {}
    }
}
