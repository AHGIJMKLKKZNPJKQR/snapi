pub mod axios;
pub mod fetch;

use indexmap::IndexMap;
use snapi_core::error::SnapiError;
use snapi_core::generator::CollisionStrategy;
use snapi_core::ir::operation::IrOperation;
use snapi_core::ir::types::{IrObject, IrType};
use snapi_core::utils::case::{to_camel_case, to_pascal_case};

#[derive(Debug)]
pub struct RenderedResource {
    pub tag: String,
    pub class_name: String,
    pub code: String,
}

pub fn render_type(ty: &IrType) -> String {
    match ty {
        IrType::String(_) => "string".to_string(),
        IrType::Integer(_) | IrType::Float(_) => "number".to_string(),
        IrType::Boolean => "boolean".to_string(),
        IrType::Null => "null".to_string(),
        IrType::Array { items, .. } => {
            let inner = render_type(items);
            if matches!(
                items.as_ref(),
                IrType::Union(_) | IrType::Intersection(_) | IrType::Optional(_)
            ) {
                format!("({})[]", inner)
            } else {
                format!("{}[]", inner)
            }
        }
        IrType::Map(v) => format!("Record<string, {}>", render_type(v)),
        IrType::Object(o) => {
            if let Some(name) = &o.name {
                to_pascal_case(name)
            } else {
                render_inline_object(o)
            }
        }
        IrType::Optional(inner) => {
            let rendered = render_type(inner);
            if matches!(inner.as_ref(), IrType::Intersection(_)) {
                format!("({}) | null", rendered)
            } else {
                format!("{} | null", rendered)
            }
        }
        IrType::Any => "unknown".to_string(),
        IrType::Recursive(name) => to_pascal_case(name),
        IrType::Enum(e) => to_pascal_case(&e.name),
        IrType::StringLiteral(s) => format!("\"{}\"", s),
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

pub fn render_models(
    schemas: &IndexMap<String, IrType>,
    strategy: CollisionStrategy,
) -> anyhow::Result<String> {
    let mut output = String::new();
    let mut seen: IndexMap<String, &str> = IndexMap::new();
    let mut counts: IndexMap<String, usize> = IndexMap::new();
    for (name, ty) in schemas {
        let base = to_pascal_case(name);
        let ts_name = match strategy {
            CollisionStrategy::Fail => {
                if let Some(prior) = seen.get(&base) {
                    return Err(SnapiError::NameCollision(format!(
                        "in model: \"{}\" and \"{}\" both produce \"{}\"",
                        prior, name, base,
                    ))
                    .into());
                }
                seen.insert(base.clone(), name);
                base
            }
            CollisionStrategy::Suffix => {
                let n = counts.entry(base.clone()).or_insert(0);
                *n += 1;
                if *n == 1 {
                    base
                } else {
                    format!("{}{}", base, n)
                }
            }
        };
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
                    .map(|v| format!("  | \"{}\"", v.name))
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
    Ok(output)
}

pub fn render_resources(
    operations: &[IrOperation],
    strategy: CollisionStrategy,
) -> anyhow::Result<Vec<RenderedResource>> {
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

    // Detect / resolve resource class name collisions
    let mut seen_classes: IndexMap<String, &str> = IndexMap::new();
    let mut class_counts: IndexMap<String, usize> = IndexMap::new();
    let mut class_names: IndexMap<String, String> = IndexMap::new();
    for tag in by_tag.keys() {
        let base = format!("{}Resource", to_pascal_case(tag));
        let name = match strategy {
            CollisionStrategy::Fail => {
                if let Some(prior) = seen_classes.get(&base) {
                    return Err(SnapiError::NameCollision(format!(
                        "in resource: \"{}\" and \"{}\" both produce \"{}\"",
                        prior, tag, base,
                    ))
                    .into());
                }
                seen_classes.insert(base.clone(), tag);
                base
            }
            CollisionStrategy::Suffix => {
                let n = class_counts.entry(base.clone()).or_insert(0);
                *n += 1;
                if *n == 1 {
                    base
                } else {
                    format!("{}{}", base, n)
                }
            }
        };
        class_names.insert(tag.clone(), name);
    }

    let mut result = vec![];
    for (tag, ops) in &by_tag {
        let class_name = class_names[tag].clone();
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

        // Build method names, detecting / resolving collisions
        let mut seen_methods: IndexMap<String, String> = IndexMap::new(); // base → first op.id
        let mut method_counts: IndexMap<String, usize> = IndexMap::new();
        let mut op_method_names: Vec<String> = Vec::with_capacity(ops.len());
        for op in ops.iter() {
            let base_method = to_camel_case(&op.id);
            let method_name = match strategy {
                CollisionStrategy::Fail => {
                    if let Some(prior) = seen_methods.get(&base_method) {
                        return Err(SnapiError::NameCollision(format!(
                            "in method (tag \"{}\"): \"{}\" and \"{}\" both produce \"{}\"",
                            tag, prior, op.id, base_method,
                        ))
                        .into());
                    }
                    seen_methods.insert(base_method.clone(), op.id.clone());
                    base_method
                }
                CollisionStrategy::Suffix => {
                    let n = method_counts.entry(base_method.clone()).or_insert(0);
                    *n += 1;
                    if *n == 1 {
                        base_method
                    } else {
                        format!("{}{}", base_method, n)
                    }
                }
            };
            op_method_names.push(method_name);
        }

        for (op, method_name) in ops.iter().zip(op_method_names.iter()) {
            let method_name = method_name.as_str();

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
            let url_kw = if query_params.is_empty() {
                "const"
            } else {
                "let"
            };
            code.push_str(&format!(
                "    {} url = new URL(`${{this.baseUrl}}{}`).toString();\n",
                url_kw, url
            ));
            if !query_params.is_empty() {
                code.push_str("    if (query) {\n");
                code.push_str("      const params = new URLSearchParams();\n");
                for p in &query_params {
                    let ts_name = to_camel_case(&p.name);
                    code.push_str(&format!(
                        "      if (query.{} != null) params.set(\"{}\", String(query.{}));\n",
                        ts_name, p.name, ts_name
                    ));
                }
                code.push_str("      url = url + '?' + params.toString();\n");
                code.push_str("    }\n");
            }
            code.push_str("    const response = await fetch(url, {\n");
            code.push_str(&format!("      method: \"{}\",\n", method_str));
            if has_body {
                code.push_str(
                    "      headers: { \"Content-Type\": \"application/json\", ...this.headers },\n",
                );
                code.push_str("      body: JSON.stringify(body),\n");
            } else {
                code.push_str("      headers: { ...this.headers },\n");
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
        result.push(RenderedResource {
            tag: tag.clone(),
            class_name,
            code,
        });
    }
    Ok(result)
}

fn collect_type_names(ty: &IrType, names: &mut Vec<String>) {
    match ty {
        IrType::Object(o) => {
            if let Some(name) = &o.name {
                names.push(to_pascal_case(name));
            } else {
                for field in o.fields.values() {
                    collect_type_names(&field.ty, names);
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use snapi_core::generator::CollisionStrategy;
    use snapi_core::ir::operation::{HttpMethod, IrOperation};
    use snapi_core::ir::types::{IrIntConstraints, IrObject, IrStringConstraints, IrType};

    fn str_ty() -> IrType {
        IrType::String(IrStringConstraints {
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
        })
    }

    fn num_ty() -> IrType {
        IrType::Integer(IrIntConstraints {
            minimum: None,
            maximum: None,
            format: None,
        })
    }

    fn named_object(name: &str) -> IrObject {
        IrObject {
            name: Some(name.to_string()),
            fields: IndexMap::new(),
        }
    }

    #[test]
    fn array_of_union_needs_parens() {
        let ty = IrType::Array {
            items: Box::new(IrType::Union(vec![str_ty(), num_ty()])),
            min: None,
            max: None,
        };
        assert_eq!(render_type(&ty), "(string | number)[]");
    }

    #[test]
    fn array_of_optional_needs_parens() {
        let ty = IrType::Array {
            items: Box::new(IrType::Optional(Box::new(str_ty()))),
            min: None,
            max: None,
        };
        assert_eq!(render_type(&ty), "(string | null)[]");
    }

    #[test]
    fn array_of_intersection_needs_parens() {
        let ty = IrType::Array {
            items: Box::new(IrType::Intersection(vec![
                named_object("Foo"),
                named_object("Bar"),
            ])),
            min: None,
            max: None,
        };
        assert_eq!(render_type(&ty), "(Foo & Bar)[]");
    }

    #[test]
    fn optional_intersection_needs_parens() {
        let ty = IrType::Optional(Box::new(IrType::Intersection(vec![
            named_object("Foo"),
            named_object("Bar"),
        ])));
        assert_eq!(render_type(&ty), "(Foo & Bar) | null");
    }

    #[test]
    fn array_of_simple_type_no_parens() {
        let ty = IrType::Array {
            items: Box::new(str_ty()),
            min: None,
            max: None,
        };
        assert_eq!(render_type(&ty), "string[]");
    }

    #[test]
    fn optional_simple_type_no_parens() {
        let ty = IrType::Optional(Box::new(str_ty()));
        assert_eq!(render_type(&ty), "string | null");
    }

    #[test]
    fn optional_union_flattens() {
        // Optional(Union(A, B)) → A | B | null (union absorption, no extra parens needed)
        let ty = IrType::Optional(Box::new(IrType::Union(vec![str_ty(), num_ty()])));
        assert_eq!(render_type(&ty), "string | number | null");
    }

    #[test]
    fn enum_renders_original_string_values() {
        use snapi_core::ir::types::{IrEnum, IrEnumVariant};
        let ty = IrType::Enum(IrEnum {
            name: "Status".to_string(),
            variants: vec![
                IrEnumVariant {
                    name: "active".to_string(),
                    ty: str_ty(),
                },
                IrEnumVariant {
                    name: "pending".to_string(),
                    ty: str_ty(),
                },
            ],
            discriminator: None,
        });
        let mut schemas = IndexMap::new();
        schemas.insert("Status".to_string(), ty);
        let output = render_models(&schemas, CollisionStrategy::Fail).unwrap();
        assert!(
            output.contains("\"active\""),
            "must render original value; got:\n{output}"
        );
        assert!(
            output.contains("\"pending\""),
            "must render original value; got:\n{output}"
        );
        assert!(
            !output.contains("ACTIVE"),
            "must not transform to SCREAMING_SNAKE_CASE; got:\n{output}"
        );
    }

    #[test]
    fn collect_type_names_recurses_into_unnamed_object_fields() {
        use snapi_core::ir::types::IrField;
        // Unnamed inline object whose field references a named type — the named
        // type must be collected so the generator can emit the correct import.
        let mut fields = IndexMap::new();
        fields.insert(
            "resolution".to_string(),
            IrField {
                ty: IrType::Object(named_object("Resolution")),
                required: true,
                description: None,
            },
        );
        let inline_obj = IrType::Object(IrObject { name: None, fields });
        let mut names = vec![];
        collect_type_names(&inline_obj, &mut names);
        assert!(
            names.contains(&"Resolution".to_string()),
            "collect_type_names must recurse into unnamed object fields; got {names:?}"
        );
    }

    // ---------------------------------------------------------------------------
    // Collision detection
    // ---------------------------------------------------------------------------

    fn simple_op(id: &str, tag: &str) -> IrOperation {
        IrOperation {
            id: id.to_string(),
            path: format!("/{}", id),
            method: HttpMethod::Get,
            summary: None,
            description: None,
            tags: vec![tag.to_string()],
            params: vec![],
            body: None,
            responses: vec![],
            deprecated: false,
        }
    }

    #[test]
    fn model_name_collision_fails_by_default() {
        let mut schemas = IndexMap::new();
        schemas.insert("hello_world".to_string(), str_ty());
        schemas.insert("hello-world".to_string(), str_ty());
        let err = render_models(&schemas, CollisionStrategy::Fail).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello_world"),
            "error should name first schema; got: {msg}"
        );
        assert!(
            msg.contains("hello-world"),
            "error should name second schema; got: {msg}"
        );
    }

    #[test]
    fn model_name_collision_suffixes_with_suffix_strategy() {
        let mut schemas = IndexMap::new();
        schemas.insert("hello_world".to_string(), str_ty());
        schemas.insert("hello-world".to_string(), str_ty());
        let output = render_models(&schemas, CollisionStrategy::Suffix).unwrap();
        assert!(
            output.contains("HelloWorld ") || output.contains("HelloWorld\n"),
            "first schema should keep base name; got:\n{output}"
        );
        assert!(
            output.contains("HelloWorld2"),
            "second schema should get suffix; got:\n{output}"
        );
    }

    #[test]
    fn resource_class_collision_fails_by_default() {
        let ops = vec![
            simple_op("list", "hello_world"),
            simple_op("create", "hello-world"),
        ];
        let err = render_resources(&ops, CollisionStrategy::Fail).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello_world"),
            "error should name first tag; got: {msg}"
        );
        assert!(
            msg.contains("hello-world"),
            "error should name second tag; got: {msg}"
        );
    }

    #[test]
    fn resource_class_collision_suffixes_class_names() {
        let ops = vec![
            simple_op("list", "hello_world"),
            simple_op("create", "hello-world"),
        ];
        let resources = render_resources(&ops, CollisionStrategy::Suffix).unwrap();
        let class_names: Vec<&str> = resources.iter().map(|r| r.class_name.as_str()).collect();
        assert!(
            class_names.contains(&"HelloWorldResource"),
            "first tag should keep base class name; got: {class_names:?}"
        );
        assert!(
            class_names.contains(&"HelloWorldResource2"),
            "second tag should get suffixed class name; got: {class_names:?}"
        );
    }

    #[test]
    fn method_name_collision_fails_by_default() {
        // "get_thing" and "getThing" both normalize to "getThing"
        let ops = vec![
            simple_op("get_thing", "items"),
            simple_op("getThing", "items"),
        ];
        let err = render_resources(&ops, CollisionStrategy::Fail).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("get_thing"),
            "error should name first op id; got: {msg}"
        );
        assert!(
            msg.contains("getThing"),
            "error should name second op id; got: {msg}"
        );
    }

    #[test]
    fn method_name_collision_suffixes_method_names() {
        let ops = vec![
            simple_op("get_thing", "items"),
            simple_op("getThing", "items"),
        ];
        let resources = render_resources(&ops, CollisionStrategy::Suffix).unwrap();
        let code = &resources[0].code;
        assert!(
            code.contains("async getThing("),
            "first method keeps base name; got:\n{code}"
        );
        assert!(
            code.contains("async getThing2("),
            "second method gets suffix; got:\n{code}"
        );
    }
}
