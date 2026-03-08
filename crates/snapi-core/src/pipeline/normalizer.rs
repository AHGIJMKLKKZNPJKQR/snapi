use super::resolver::ResolvedSpec;
use crate::ir::*;
use indexmap::IndexMap;
use oas3::spec::{ObjectOrReference, ObjectSchema, SchemaType, SchemaTypeSet};
use std::collections::{BTreeMap, HashSet};

pub fn normalize(resolved: ResolvedSpec) -> anyhow::Result<IrApi> {
    let spec = &resolved.inner;
    let info = &spec.info;

    let title = info.title.clone();
    let version = info.version.clone();
    let description = info.description.clone();

    // Servers
    let servers = spec
        .servers
        .iter()
        .map(|s| IrServer {
            url: s.url.clone(),
            description: s.description.clone(),
        })
        .collect();

    // Schemas
    let mut schemas = IndexMap::new();
    let mut visiting: HashSet<String> = HashSet::new();

    if let Some(components) = &spec.components {
        for (name, schema_ref) in &components.schemas {
            if let ObjectOrReference::Object(schema) = schema_ref {
                visiting.insert(name.clone());
                let ty = normalize_schema(schema, Some(name), &mut visiting, &components.schemas)?;
                visiting.remove(name);
                schemas.insert(name.clone(), ty);
            }
        }
    }

    // Operations
    let mut operations = vec![];
    if let Some(paths) = &spec.paths {
        for (path_str, path_item) in paths {
            extract_operations(path_str, path_item, &mut operations, &resolved)?;
        }
    }

    // Auth schemes
    let mut auth_schemes = IndexMap::new();
    if let Some(components) = &spec.components {
        for (name, sec_ref) in &components.security_schemes {
            if let ObjectOrReference::Object(sec) = sec_ref {
                if let Some(scheme) = normalize_security_scheme(sec) {
                    auth_schemes.insert(name.clone(), scheme);
                }
            }
        }
    }

    Ok(IrApi {
        title,
        version,
        description,
        servers,
        schemas,
        operations,
        auth_schemes,
        webhooks: vec![],
    })
}

fn normalize_schema(
    schema: &ObjectSchema,
    name: Option<&str>,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    // Check for allOf
    if !schema.all_of.is_empty() {
        return normalize_all_of(&schema.all_of, name, visiting, all_schemas);
    }

    // Check for oneOf
    if !schema.one_of.is_empty() {
        return normalize_one_of(schema, name, visiting, all_schemas);
    }

    // Check for anyOf
    if !schema.any_of.is_empty() {
        return normalize_any_of(&schema.any_of, name, visiting, all_schemas);
    }

    // String enum: type: string (or no type) + enum: [...] → IrType::Enum
    // This covers the common pattern `type: string\nenum: [a, b, c]`.
    if !schema.enum_values.is_empty() {
        let enum_name = name.unwrap_or("Enum").to_string();
        let variants = schema
            .enum_values
            .iter()
            .map(|s| IrEnumVariant {
                name: crate::utils::case::to_pascal_case(s),
                ty: IrType::String(IrStringConstraints {
                    min_length: None,
                    max_length: None,
                    pattern: None,
                    format: None,
                }),
            })
            .collect();
        return Ok(IrType::Enum(IrEnum {
            name: enum_name,
            variants,
            discriminator: None,
        }));
    }

    // Determine type(s)
    let types = get_schema_types(schema);

    // Check for nullable pattern: type: ["X", "null"]
    let non_null: Vec<_> = types.iter().filter(|t| **t != SchemaType::Null).collect();
    if types.contains(&SchemaType::Null) && non_null.len() == 1 {
        let inner_type = build_simple_type(non_null[0], schema, name, visiting, all_schemas)?;
        return Ok(IrType::Optional(Box::new(inner_type)));
    }

    if types.len() > 1 {
        // Union of multiple types
        let mut variants = vec![];
        for t in &types {
            let ty = build_simple_type(t, schema, name, visiting, all_schemas)?;
            variants.push(ty);
        }
        return Ok(IrType::Union(variants));
    }

    let type_val = types.first().copied().unwrap_or(SchemaType::Object);
    build_simple_type(&type_val, schema, name, visiting, all_schemas)
}

fn get_schema_types(schema: &ObjectSchema) -> Vec<SchemaType> {
    match &schema.schema_type {
        Some(SchemaTypeSet::Single(t)) => vec![*t],
        Some(SchemaTypeSet::Multiple(ts)) => ts.clone(),
        None => vec![],
    }
}

fn build_simple_type(
    type_val: &SchemaType,
    schema: &ObjectSchema,
    name: Option<&str>,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    match type_val {
        SchemaType::String => Ok(IrType::String(IrStringConstraints {
            min_length: schema.min_length,
            max_length: schema.max_length,
            pattern: schema.pattern.clone(),
            format: schema.format.clone(),
        })),
        SchemaType::Integer => Ok(IrType::Integer(IrIntConstraints {
            minimum: schema.minimum.as_ref().and_then(|v| v.as_i64()),
            maximum: schema.maximum.as_ref().and_then(|v| v.as_i64()),
            format: schema.format.clone(),
        })),
        SchemaType::Number => Ok(IrType::Float(IrFloatConstraints {
            minimum: schema.minimum.as_ref().and_then(|v| v.as_f64()),
            maximum: schema.maximum.as_ref().and_then(|v| v.as_f64()),
        })),
        SchemaType::Boolean => Ok(IrType::Boolean),
        SchemaType::Null => Ok(IrType::Null),
        SchemaType::Array => {
            let items = if let Some(items_ref) = &schema.items {
                match items_ref.as_ref() {
                    ObjectOrReference::Object(items_schema) => {
                        normalize_schema(items_schema, None, visiting, all_schemas)?
                    }
                    ObjectOrReference::Ref { ref_path } => {
                        resolve_ref_type(ref_path, visiting, all_schemas)?
                    }
                }
            } else {
                IrType::Any
            };
            Ok(IrType::Array {
                items: Box::new(items),
                min: schema.min_items,
                max: schema.max_items,
            })
        }
        SchemaType::Object => normalize_object(schema, name, visiting, all_schemas),
    }
}

fn normalize_object(
    schema: &ObjectSchema,
    name: Option<&str>,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    // Check for additionalProperties (Map type)
    if let Some(additional) = &schema.additional_properties {
        use oas3::spec::Schema;
        return match additional {
            Schema::Object(oor) => match oor.as_ref() {
                ObjectOrReference::Object(ap_schema) => {
                    let value_type = normalize_schema(ap_schema, None, visiting, all_schemas)?;
                    Ok(IrType::Map(Box::new(value_type)))
                }
                ObjectOrReference::Ref { ref_path } => {
                    let value_type = resolve_ref_type(ref_path, visiting, all_schemas)?;
                    Ok(IrType::Map(Box::new(value_type)))
                }
            },
            Schema::Boolean(_) => Ok(IrType::Map(Box::new(IrType::Any))),
        };
    }

    // Regular object
    let required_fields: HashSet<String> = schema.required.iter().cloned().collect();

    let mut fields = IndexMap::new();
    for (field_name, field_ref) in &schema.properties {
        let (field_schema, field_ty) = match field_ref {
            ObjectOrReference::Object(fs) => {
                let ty = normalize_schema(fs, None, visiting, all_schemas)?;
                (Some(fs), ty)
            }
            ObjectOrReference::Ref { ref_path } => {
                let ty = resolve_ref_type(ref_path, visiting, all_schemas)?;
                (None, ty)
            }
        };
        let required = required_fields.contains(field_name);
        let description = field_schema.and_then(|fs| fs.description.clone());
        fields.insert(
            field_name.clone(),
            IrField {
                ty: field_ty,
                required,
                description,
            },
        );
    }

    Ok(IrType::Object(IrObject {
        name: name.map(|s| s.to_string()),
        fields,
    }))
}

fn normalize_all_of(
    all_of: &[ObjectOrReference<ObjectSchema>],
    name: Option<&str>,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    let mut objects = vec![];
    for schema_ref in all_of {
        match schema_ref {
            ObjectOrReference::Object(s) => {
                let ty = normalize_schema(s, None, visiting, all_schemas)?;
                if let IrType::Object(obj) = ty {
                    objects.push(obj);
                }
            }
            ObjectOrReference::Ref { ref_path } => {
                let ty = resolve_ref_type(ref_path, visiting, all_schemas)?;
                if let IrType::Object(obj) = ty {
                    objects.push(obj);
                }
            }
        }
    }

    if objects.len() == 1 {
        return Ok(IrType::Object(objects.into_iter().next().unwrap()));
    }

    // Try to merge into a single object
    let mut merged_fields = IndexMap::new();
    for obj in &objects {
        for (k, v) in &obj.fields {
            merged_fields.insert(k.clone(), v.clone());
        }
    }
    Ok(IrType::Object(IrObject {
        name: name.map(|s| s.to_string()),
        fields: merged_fields,
    }))
}

fn normalize_one_of(
    schema: &ObjectSchema,
    _name: Option<&str>,

    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    // Without discriminator: Union
    let mut variants = vec![];
    for schema_ref in &schema.one_of {
        let ty = match schema_ref {
            ObjectOrReference::Object(s) => normalize_schema(s, None, visiting, all_schemas)?,
            ObjectOrReference::Ref { ref_path } => {
                resolve_ref_type(ref_path, visiting, all_schemas)?
            }
        };
        variants.push(ty);
    }
    Ok(IrType::Union(variants))
}

fn normalize_any_of(
    any_of: &[ObjectOrReference<ObjectSchema>],
    _name: Option<&str>,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    let mut variants = vec![];
    for schema_ref in any_of {
        let ty = match schema_ref {
            ObjectOrReference::Object(s) => normalize_schema(s, None, visiting, all_schemas)?,
            ObjectOrReference::Ref { ref_path } => {
                resolve_ref_type(ref_path, visiting, all_schemas)?
            }
        };
        variants.push(ty);
    }
    Ok(IrType::Union(variants))
}

fn resolve_ref_type(
    ref_path: &str,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    // ref_path like "#/components/schemas/Foo"
    let name = ref_path.split('/').next_back().unwrap_or(ref_path);

    // Circular reference check
    if visiting.contains(name) {
        return Ok(IrType::Recursive(name.to_string()));
    }

    if let Some(schema_ref) = all_schemas.get(name) {
        match schema_ref {
            ObjectOrReference::Object(schema) => {
                visiting.insert(name.to_string());
                let ty = normalize_schema(schema, Some(name), visiting, all_schemas)?;
                visiting.remove(name);
                Ok(ty)
            }
            ObjectOrReference::Ref {
                ref_path: inner_ref,
            } => resolve_ref_type(inner_ref, visiting, all_schemas),
        }
    } else {
        // Unknown ref — return a named recursive type
        Ok(IrType::Recursive(name.to_string()))
    }
}

fn extract_operations(
    path_str: &str,
    path_item: &oas3::spec::PathItem,
    operations: &mut Vec<IrOperation>,
    resolved: &ResolvedSpec,
) -> anyhow::Result<()> {
    let all_schemas = resolved
        .inner
        .components
        .as_ref()
        .map(|c| c.schemas.clone())
        .unwrap_or_default();

    let op_pairs: Vec<(HttpMethod, Option<&oas3::spec::Operation>)> = vec![
        (HttpMethod::Get, path_item.get.as_ref()),
        (HttpMethod::Post, path_item.post.as_ref()),
        (HttpMethod::Put, path_item.put.as_ref()),
        (HttpMethod::Patch, path_item.patch.as_ref()),
        (HttpMethod::Delete, path_item.delete.as_ref()),
        (HttpMethod::Head, path_item.head.as_ref()),
        (HttpMethod::Options, path_item.options.as_ref()),
    ];

    for (method, maybe_op) in op_pairs {
        if let Some(op) = maybe_op {
            let method_str = format!("{:?}", method).to_lowercase();
            let op_id = op.operation_id.clone().unwrap_or_else(|| {
                format!(
                    "{}_{}",
                    method_str,
                    path_str
                        .trim_start_matches('/')
                        .replace('/', "_")
                        .replace(['{', '}'], "")
                )
            });

            let mut params = vec![];
            for param_ref in &op.parameters {
                if let ObjectOrReference::Object(p) = param_ref {
                    use oas3::spec::ParameterIn;
                    let location = match p.location {
                        ParameterIn::Query => ParamLocation::Query,
                        ParameterIn::Header => ParamLocation::Header,
                        ParameterIn::Cookie => ParamLocation::Cookie,
                        ParameterIn::Path => ParamLocation::Path,
                    };
                    let ty = if let Some(schema_ref) = &p.schema {
                        match schema_ref {
                            ObjectOrReference::Object(s) => {
                                let mut vis = resolved.visited_schemas.clone();
                                normalize_schema(s, None, &mut vis, &all_schemas)
                                    .unwrap_or(IrType::Any)
                            }
                            ObjectOrReference::Ref { ref_path } => {
                                let mut vis = resolved.visited_schemas.clone();
                                resolve_ref_type(ref_path, &mut vis, &all_schemas)
                                    .unwrap_or(IrType::Any)
                            }
                        }
                    } else {
                        IrType::Any
                    };
                    params.push(IrParam {
                        name: p.name.clone(),
                        location,
                        required: p.required.unwrap_or(false),
                        ty,
                        description: p.description.clone(),
                    });
                }
            }

            // Request body
            let body = if let Some(body_ref) = &op.request_body {
                match body_ref {
                    ObjectOrReference::Object(body) => {
                        let (content_type, ty) = body
                            .content
                            .iter()
                            .next()
                            .and_then(|(ct, media)| {
                                let t = media.schema.as_ref().and_then(|sr| match sr {
                                    ObjectOrReference::Object(s) => {
                                        let mut vis = resolved.visited_schemas.clone();
                                        normalize_schema(s, None, &mut vis, &all_schemas).ok()
                                    }
                                    ObjectOrReference::Ref { ref_path } => {
                                        let mut vis = resolved.visited_schemas.clone();
                                        resolve_ref_type(ref_path, &mut vis, &all_schemas).ok()
                                    }
                                })?;
                                Some((ct.clone(), t))
                            })
                            .unwrap_or_else(|| ("application/json".to_string(), IrType::Any));
                        Some(IrRequestBody {
                            required: body.required.unwrap_or(false),
                            content_type,
                            ty,
                            description: body.description.clone(),
                        })
                    }
                    _ => None,
                }
            } else {
                None
            };

            // Responses
            let mut responses = vec![];
            if let Some(resp_map) = &op.responses {
                for (status_str, response_ref) in resp_map {
                    let status: u16 = status_str.parse().unwrap_or(200);
                    let response = match response_ref {
                        ObjectOrReference::Object(r) => r,
                        _ => continue,
                    };
                    let (content_type, ty) = response
                        .content
                        .iter()
                        .next()
                        .and_then(|(ct, media)| {
                            let t = media.schema.as_ref().and_then(|sr| match sr {
                                ObjectOrReference::Object(s) => {
                                    let mut vis = resolved.visited_schemas.clone();
                                    normalize_schema(s, None, &mut vis, &all_schemas).ok()
                                }
                                ObjectOrReference::Ref { ref_path } => {
                                    let mut vis = resolved.visited_schemas.clone();
                                    resolve_ref_type(ref_path, &mut vis, &all_schemas).ok()
                                }
                            })?;
                            Some((ct.clone(), t))
                        })
                        .map(|(ct, t)| (Some(ct), Some(t)))
                        .unwrap_or((None, None));

                    responses.push(IrResponse {
                        status,
                        content_type,
                        ty,
                        description: response.description.clone(),
                    });
                }
            }

            let tags: Vec<String> = op.tags.clone();

            operations.push(IrOperation {
                id: op_id,
                path: path_str.to_string(),
                method,
                summary: op.summary.clone(),
                description: op.description.clone(),
                tags,
                params,
                body,
                responses,
                deprecated: op.deprecated.unwrap_or(false),
            });
        }
    }
    Ok(())
}

fn normalize_security_scheme(sec: &oas3::spec::SecurityScheme) -> Option<IrAuthScheme> {
    use oas3::spec::SecurityScheme;
    match sec {
        SecurityScheme::ApiKey { name, location, .. } => Some(IrAuthScheme::ApiKey {
            name: name.clone(),
            location: location.clone(),
        }),
        SecurityScheme::Http { scheme, .. } => Some(IrAuthScheme::Bearer {
            scheme: scheme.clone(),
        }),
        SecurityScheme::OAuth2 { .. } => Some(IrAuthScheme::OAuth2 { flows: vec![] }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_string_schema() -> ObjectSchema {
        let mut s = ObjectSchema::default();
        s.schema_type = Some(SchemaTypeSet::Single(SchemaType::String));
        s
    }

    fn make_object_schema(
        properties: BTreeMap<String, ObjectOrReference<ObjectSchema>>,
        required: Vec<String>,
    ) -> ObjectSchema {
        let mut s = ObjectSchema::default();
        s.schema_type = Some(SchemaTypeSet::Single(SchemaType::Object));
        s.properties = properties;
        s.required = required;
        s
    }

    #[test]
    fn test_normalize_string() {
        let schema = make_string_schema();
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();
        assert!(matches!(ty, IrType::String(_)));
    }

    #[test]
    fn test_normalize_nullable_string() {
        let mut schema = ObjectSchema::default();
        schema.schema_type = Some(SchemaTypeSet::Multiple(vec![
            SchemaType::String,
            SchemaType::Null,
        ]));
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();
        assert!(matches!(ty, IrType::Optional(_)));
        if let IrType::Optional(inner) = ty {
            assert!(matches!(*inner, IrType::String(_)));
        }
    }

    #[test]
    fn test_normalize_integer() {
        let mut schema = ObjectSchema::default();
        schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::Integer));
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();
        assert!(matches!(ty, IrType::Integer(_)));
    }

    #[test]
    fn test_normalize_boolean() {
        let mut schema = ObjectSchema::default();
        schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::Boolean));
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();
        assert!(matches!(ty, IrType::Boolean));
    }

    #[test]
    fn test_normalize_object_with_required_fields() {
        let mut properties = BTreeMap::new();
        properties.insert(
            "id".to_string(),
            ObjectOrReference::Object({
                let mut s = ObjectSchema::default();
                s.schema_type = Some(SchemaTypeSet::Single(SchemaType::Integer));
                s
            }),
        );
        properties.insert(
            "name".to_string(),
            ObjectOrReference::Object(make_string_schema()),
        );

        let schema = make_object_schema(properties, vec!["id".to_string()]);
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, Some("Item"), &mut visiting, &all_schemas).unwrap();

        if let IrType::Object(obj) = ty {
            assert_eq!(obj.name, Some("Item".to_string()));
            assert!(obj.fields["id"].required);
            assert!(!obj.fields["name"].required);
        } else {
            panic!("expected Object, got {:?}", ty);
        }
    }

    #[test]
    fn test_normalize_enum_values() {
        let mut schema = ObjectSchema::default();
        schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::Object));
        schema.enum_values = vec![
            "active".to_string(),
            "inactive".to_string(),
            "pending".to_string(),
        ];
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, Some("Status"), &mut visiting, &all_schemas).unwrap();

        if let IrType::Enum(e) = ty {
            assert_eq!(e.name, "Status");
            assert_eq!(e.variants.len(), 3);
            let names: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
            assert!(names.contains(&"Active"));
            assert!(names.contains(&"Inactive"));
            assert!(names.contains(&"Pending"));
        } else {
            panic!("expected Enum, got {:?}", ty);
        }
    }

    #[test]
    fn test_normalize_array_with_items() {
        let mut schema = ObjectSchema::default();
        schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::Array));
        schema.items = Some(Box::new(ObjectOrReference::Object(make_string_schema())));
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();

        if let IrType::Array { items, min, max } = ty {
            assert!(matches!(*items, IrType::String(_)));
            assert_eq!(min, None);
            assert_eq!(max, None);
        } else {
            panic!("expected Array, got {:?}", ty);
        }
    }

    #[test]
    fn test_normalize_array_without_items_is_any() {
        let mut schema = ObjectSchema::default();
        schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::Array));
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();

        if let IrType::Array { items, .. } = ty {
            assert!(matches!(*items, IrType::Any));
        } else {
            panic!("expected Array");
        }
    }

    #[test]
    fn test_normalize_one_of_produces_union() {
        let mut schema = ObjectSchema::default();
        schema.one_of = vec![
            ObjectOrReference::Object(make_string_schema()),
            ObjectOrReference::Object({
                let mut s = ObjectSchema::default();
                s.schema_type = Some(SchemaTypeSet::Single(SchemaType::Integer));
                s
            }),
        ];
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();

        if let IrType::Union(variants) = ty {
            assert_eq!(variants.len(), 2);
            assert!(matches!(variants[0], IrType::String(_)));
            assert!(matches!(variants[1], IrType::Integer(_)));
        } else {
            panic!("expected Union, got {:?}", ty);
        }
    }

    #[test]
    fn test_normalize_any_of_produces_union() {
        let mut schema = ObjectSchema::default();
        schema.any_of = vec![
            ObjectOrReference::Object(make_string_schema()),
            ObjectOrReference::Object({
                let mut s = ObjectSchema::default();
                s.schema_type = Some(SchemaTypeSet::Single(SchemaType::Boolean));
                s
            }),
        ];
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();

        if let IrType::Union(variants) = ty {
            assert_eq!(variants.len(), 2);
            assert!(matches!(variants[0], IrType::String(_)));
            assert!(matches!(variants[1], IrType::Boolean));
        } else {
            panic!("expected Union, got {:?}", ty);
        }
    }

    #[test]
    fn test_normalize_all_of_merges_fields() {
        let mut base_props = BTreeMap::new();
        base_props.insert(
            "id".to_string(),
            ObjectOrReference::Object({
                let mut s = ObjectSchema::default();
                s.schema_type = Some(SchemaTypeSet::Single(SchemaType::Integer));
                s
            }),
        );
        let mut base = ObjectSchema::default();
        base.schema_type = Some(SchemaTypeSet::Single(SchemaType::Object));
        base.properties = base_props;
        base.required = vec!["id".to_string()];

        let mut ext_props = BTreeMap::new();
        ext_props.insert(
            "role".to_string(),
            ObjectOrReference::Object(make_string_schema()),
        );
        let mut ext = ObjectSchema::default();
        ext.schema_type = Some(SchemaTypeSet::Single(SchemaType::Object));
        ext.properties = ext_props;

        let mut schema = ObjectSchema::default();
        schema.all_of = vec![
            ObjectOrReference::Object(base),
            ObjectOrReference::Object(ext),
        ];

        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, Some("Admin"), &mut visiting, &all_schemas).unwrap();

        if let IrType::Object(obj) = ty {
            assert!(obj.fields.contains_key("id"));
            assert!(obj.fields.contains_key("role"));
        } else {
            panic!("expected merged Object, got {:?}", ty);
        }
    }

    #[test]
    fn test_circular_ref_produces_recursive() {
        // Node schema that references itself via a $ref
        let mut properties = BTreeMap::new();
        properties.insert(
            "child".to_string(),
            ObjectOrReference::Ref {
                ref_path: "#/components/schemas/Node".to_string(),
            },
        );
        let mut node_schema = ObjectSchema::default();
        node_schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::Object));
        node_schema.properties = properties;

        let mut all_schemas = BTreeMap::new();
        all_schemas.insert("Node".to_string(), ObjectOrReference::Object(node_schema));

        // Simulate visiting "Node" so the self-ref is circular
        let mut visiting = HashSet::new();
        visiting.insert("Node".to_string());

        let child_ref_ty =
            resolve_ref_type("#/components/schemas/Node", &mut visiting, &all_schemas).unwrap();

        assert!(matches!(child_ref_ty, IrType::Recursive(ref name) if name == "Node"));
    }

    #[test]
    fn test_normalize_map_type() {
        use oas3::spec::Schema;
        let mut schema = ObjectSchema::default();
        schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::Object));
        schema.additional_properties = Some(Schema::Object(Box::new(ObjectOrReference::Object(
            make_string_schema(),
        ))));

        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();

        if let IrType::Map(value_ty) = ty {
            assert!(matches!(*value_ty, IrType::String(_)));
        } else {
            panic!("expected Map, got {:?}", ty);
        }
    }

    #[test]
    fn test_normalize_ref_to_known_schema() {
        let mut all_schemas = BTreeMap::new();
        all_schemas.insert(
            "MyType".to_string(),
            ObjectOrReference::Object(make_string_schema()),
        );

        let mut visiting = HashSet::new();
        let ty =
            resolve_ref_type("#/components/schemas/MyType", &mut visiting, &all_schemas).unwrap();
        assert!(matches!(ty, IrType::String(_)));
    }

    #[test]
    fn test_normalize_ref_to_unknown_schema_is_recursive() {
        let all_schemas = BTreeMap::new();
        let mut visiting = HashSet::new();
        let ty =
            resolve_ref_type("#/components/schemas/Unknown", &mut visiting, &all_schemas).unwrap();
        assert!(matches!(ty, IrType::Recursive(ref name) if name == "Unknown"));
    }
}
