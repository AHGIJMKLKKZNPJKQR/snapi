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

    let servers = spec
        .servers
        .iter()
        .map(|s| IrServer {
            url: s.url.clone(),
            description: s.description.clone(),
        })
        .collect();

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

    let mut operations = vec![];
    if let Some(paths) = &spec.paths {
        for (path_str, path_item) in paths {
            extract_operations(path_str, path_item, &mut operations, &resolved)?;
        }
    }

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
    if !schema.all_of.is_empty() {
        return normalize_all_of(&schema.all_of, name, visiting, all_schemas);
    }
    if !schema.one_of.is_empty() {
        return normalize_one_of(schema, name, visiting, all_schemas);
    }
    if !schema.any_of.is_empty() {
        return normalize_any_of(&schema.any_of, name, visiting, all_schemas);
    }
    if !schema.enum_values.is_empty() {
        return Ok(match name {
            Some(n) => normalize_named_enum(schema, n),
            None => normalize_inline_enum(schema),
        });
    }

    let types = get_schema_types(schema);
    if let Some(ty) = try_build_nullable(&types, schema, name, visiting, all_schemas)? {
        return Ok(ty);
    }
    if types.len() > 1 {
        let variants = types
            .iter()
            .map(|t| build_simple_type(t, schema, name, visiting, all_schemas))
            .collect::<anyhow::Result<Vec<_>>>()?;
        return Ok(IrType::Union(variants));
    }

    let type_val = types.first().copied().unwrap_or(SchemaType::Object);
    build_simple_type(&type_val, schema, name, visiting, all_schemas)
}

fn normalize_named_enum(schema: &ObjectSchema, name: &str) -> IrType {
    let variants = schema
        .enum_values
        .iter()
        .map(|s| IrEnumVariant {
            name: s.as_str().unwrap_or("").to_string(),
            ty: IrType::String(IrStringConstraints {
                min_length: None,
                max_length: None,
                pattern: None,
                format: None,
            }),
        })
        .collect();
    IrType::Enum(IrEnum {
        name: name.to_string(),
        variants,
        discriminator: None,
    })
}

fn normalize_inline_enum(schema: &ObjectSchema) -> IrType {
    let literals: Vec<IrType> = schema
        .enum_values
        .iter()
        .filter_map(|v| v.as_str().map(|s| IrType::StringLiteral(s.to_string())))
        .collect();
    match literals.len() {
        0 => IrType::Any,
        1 => literals.into_iter().next().unwrap(),
        _ => IrType::Union(literals),
    }
}

fn try_build_nullable(
    types: &[SchemaType],
    schema: &ObjectSchema,
    name: Option<&str>,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<Option<IrType>> {
    if !types.contains(&SchemaType::Null) {
        return Ok(None);
    }
    let non_null: Vec<_> = types.iter().filter(|t| **t != SchemaType::Null).collect();
    if non_null.len() != 1 {
        return Ok(None);
    }
    let inner = build_simple_type(non_null[0], schema, name, visiting, all_schemas)?;
    Ok(Some(IrType::Optional(Box::new(inner))))
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
        SchemaType::Array => build_array_type(schema, visiting, all_schemas),
        SchemaType::Object => normalize_object(schema, name, visiting, all_schemas),
    }
}

fn build_array_type(
    schema: &ObjectSchema,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    let items = match &schema.items {
        None => IrType::Any,
        Some(items_schema) => resolve_items_schema(items_schema.as_ref(), visiting, all_schemas)?,
    };
    Ok(IrType::Array {
        items: Box::new(items),
        min: schema.min_items,
        max: schema.max_items,
    })
}

fn resolve_items_schema(
    items: &oas3::spec::Schema,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    use oas3::spec::Schema;
    match items {
        Schema::Object(oor) => match oor.as_ref() {
            ObjectOrReference::Object(s) => normalize_schema(s, None, visiting, all_schemas),
            ObjectOrReference::Ref { ref_path, .. } => {
                resolve_ref_type(ref_path, visiting, all_schemas)
            }
        },
        Schema::Boolean(_) => Ok(IrType::Any),
    }
}

fn normalize_object(
    schema: &ObjectSchema,
    name: Option<&str>,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    if schema.properties.is_empty() {
        if let Some(additional) = &schema.additional_properties {
            return normalize_additional_properties(additional, visiting, all_schemas);
        }
    }

    let required_fields: HashSet<String> = schema.required.iter().cloned().collect();
    let mut fields = IndexMap::new();
    for (field_name, field_ref) in &schema.properties {
        let field = normalize_object_field(
            field_name,
            field_ref,
            &required_fields,
            visiting,
            all_schemas,
        )?;
        fields.insert(field_name.clone(), field);
    }

    Ok(IrType::Object(IrObject {
        name: name.map(|s| s.to_string()),
        fields,
    }))
}

fn normalize_additional_properties(
    additional: &oas3::spec::Schema,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    use oas3::spec::Schema;
    let value_type = match additional {
        Schema::Object(oor) => match oor.as_ref() {
            ObjectOrReference::Object(ap_schema) => {
                normalize_schema(ap_schema, None, visiting, all_schemas)?
            }
            ObjectOrReference::Ref { ref_path, .. } => {
                resolve_ref_type(ref_path, visiting, all_schemas)?
            }
        },
        Schema::Boolean(_) => IrType::Any,
    };
    Ok(IrType::Map(Box::new(value_type)))
}

fn normalize_object_field(
    field_name: &str,
    field_ref: &ObjectOrReference<ObjectSchema>,
    required_fields: &HashSet<String>,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrField> {
    let (field_schema, field_ty) = match field_ref {
        ObjectOrReference::Object(fs) => {
            let ty = normalize_schema(fs, None, visiting, all_schemas)?;
            (Some(fs), ty)
        }
        ObjectOrReference::Ref { ref_path, .. } => {
            let ty = resolve_ref_type(ref_path, visiting, all_schemas)?;
            (None, ty)
        }
    };
    Ok(IrField {
        ty: field_ty,
        required: required_fields.contains(field_name),
        description: field_schema.and_then(|fs| fs.description.clone()),
    })
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
            ObjectOrReference::Ref { ref_path, .. } => {
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
    let variants = schema
        .one_of
        .iter()
        .map(|schema_ref| match schema_ref {
            ObjectOrReference::Object(s) => normalize_schema(s, None, visiting, all_schemas),
            ObjectOrReference::Ref { ref_path, .. } => {
                resolve_ref_type(ref_path, visiting, all_schemas)
            }
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(IrType::Union(variants))
}

fn normalize_any_of(
    any_of: &[ObjectOrReference<ObjectSchema>],
    _name: Option<&str>,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    let variants = any_of
        .iter()
        .map(|schema_ref| match schema_ref {
            ObjectOrReference::Object(s) => normalize_schema(s, None, visiting, all_schemas),
            ObjectOrReference::Ref { ref_path, .. } => {
                resolve_ref_type(ref_path, visiting, all_schemas)
            }
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(IrType::Union(variants))
}

fn resolve_ref_type(
    ref_path: &str,
    visiting: &mut HashSet<String>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> anyhow::Result<IrType> {
    let name = ref_path.split('/').next_back().unwrap_or(ref_path);

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
                ..
            } => resolve_ref_type(inner_ref, visiting, all_schemas),
        }
    } else {
        Ok(IrType::Recursive(name.to_string()))
    }
}

fn resolve_schema_ref(
    schema_ref: &ObjectOrReference<ObjectSchema>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> Option<IrType> {
    let mut vis = HashSet::new();
    match schema_ref {
        ObjectOrReference::Object(s) => normalize_schema(s, None, &mut vis, all_schemas).ok(),
        ObjectOrReference::Ref { ref_path, .. } => {
            resolve_ref_type(ref_path, &mut vis, all_schemas).ok()
        }
    }
}

fn resolve_media_content<'a>(
    mut content: impl Iterator<Item = (&'a String, &'a oas3::spec::MediaType)>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> Option<(String, IrType)> {
    let (ct, media) = content.next()?;
    let ty = resolve_schema_ref(media.schema.as_ref()?, all_schemas)?;
    Some((ct.clone(), ty))
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
            operations.push(extract_single_operation(method, op, path_str, &all_schemas));
        }
    }
    Ok(())
}

fn extract_single_operation(
    method: HttpMethod,
    op: &oas3::spec::Operation,
    path_str: &str,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> IrOperation {
    let op_id = build_op_id(&method, op, path_str);
    let params = extract_params(&op.parameters, all_schemas);
    let body = op
        .request_body
        .as_ref()
        .and_then(|b| extract_body(b, all_schemas));
    let responses = op
        .responses
        .as_ref()
        .map(|m| {
            m.iter()
                .filter_map(|(s, r)| extract_response(s, r, all_schemas))
                .collect()
        })
        .unwrap_or_default();

    IrOperation {
        id: op_id,
        path: path_str.to_string(),
        method,
        summary: op.summary.clone(),
        description: op.description.clone(),
        tags: op.tags.clone(),
        params,
        body,
        responses,
        deprecated: op.deprecated.unwrap_or(false),
    }
}

fn build_op_id(method: &HttpMethod, op: &oas3::spec::Operation, path_str: &str) -> String {
    op.operation_id.clone().unwrap_or_else(|| {
        let method_str = format!("{:?}", method).to_lowercase();
        format!(
            "{}_{}",
            method_str,
            path_str
                .trim_start_matches('/')
                .replace('/', "_")
                .replace(['{', '}'], "")
        )
    })
}

fn extract_params(
    params: &[ObjectOrReference<oas3::spec::Parameter>],
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> Vec<IrParam> {
    params
        .iter()
        .filter_map(|param_ref| match param_ref {
            ObjectOrReference::Object(p) => Some(extract_param(p, all_schemas)),
            _ => None,
        })
        .collect()
}

fn extract_param(
    p: &oas3::spec::Parameter,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> IrParam {
    use oas3::spec::ParameterIn;
    let location = match p.location {
        ParameterIn::Query => ParamLocation::Query,
        ParameterIn::Header => ParamLocation::Header,
        ParameterIn::Cookie => ParamLocation::Cookie,
        ParameterIn::Path => ParamLocation::Path,
    };
    let ty = p
        .schema
        .as_ref()
        .and_then(|sr| resolve_schema_ref(sr, all_schemas))
        .unwrap_or(IrType::Any);
    IrParam {
        name: p.name.clone(),
        location,
        required: p.required.unwrap_or(false),
        ty,
        description: p.description.clone(),
    }
}

fn extract_body(
    body_ref: &ObjectOrReference<oas3::spec::RequestBody>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> Option<IrRequestBody> {
    let body = match body_ref {
        ObjectOrReference::Object(b) => b,
        _ => return None,
    };
    let (content_type, ty) = resolve_media_content(body.content.iter(), all_schemas)
        .unwrap_or_else(|| ("application/json".to_string(), IrType::Any));
    Some(IrRequestBody {
        required: body.required.unwrap_or(false),
        content_type,
        ty,
        description: body.description.clone(),
    })
}

fn extract_response(
    status_str: &str,
    response_ref: &ObjectOrReference<oas3::spec::Response>,
    all_schemas: &BTreeMap<String, ObjectOrReference<ObjectSchema>>,
) -> Option<IrResponse> {
    let status: u16 = status_str.parse().unwrap_or(200);
    let response = match response_ref {
        ObjectOrReference::Object(r) => r,
        _ => return None,
    };
    let (content_type, ty) = resolve_media_content(response.content.iter(), all_schemas)
        .map(|(ct, t)| (Some(ct), Some(t)))
        .unwrap_or((None, None));
    Some(IrResponse {
        status,
        content_type,
        ty,
        description: response.description.clone(),
    })
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
        ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::String)),
            ..Default::default()
        }
    }

    fn make_object_schema(
        properties: BTreeMap<String, ObjectOrReference<ObjectSchema>>,
        required: Vec<String>,
    ) -> ObjectSchema {
        ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Object)),
            properties,
            required,
            ..Default::default()
        }
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
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Multiple(vec![
                SchemaType::String,
                SchemaType::Null,
            ])),
            ..Default::default()
        };
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
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Integer)),
            ..Default::default()
        };
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();
        assert!(matches!(ty, IrType::Integer(_)));
    }

    #[test]
    fn test_normalize_boolean() {
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Boolean)),
            ..Default::default()
        };
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
            ObjectOrReference::Object(ObjectSchema {
                schema_type: Some(SchemaTypeSet::Single(SchemaType::Integer)),
                ..Default::default()
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
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Object)),
            enum_values: vec![
                serde_json::Value::String("active".to_string()),
                serde_json::Value::String("inactive".to_string()),
                serde_json::Value::String("pending".to_string()),
            ],
            ..Default::default()
        };
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, Some("Status"), &mut visiting, &all_schemas).unwrap();

        if let IrType::Enum(e) = ty {
            assert_eq!(e.name, "Status");
            assert_eq!(e.variants.len(), 3);
            let names: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
            // Variant names must preserve the original wire values, not PascalCase them.
            assert!(names.contains(&"active"), "got {names:?}");
            assert!(names.contains(&"inactive"), "got {names:?}");
            assert!(names.contains(&"pending"), "got {names:?}");
        } else {
            panic!("expected Enum, got {:?}", ty);
        }
    }

    #[test]
    fn test_normalize_object_with_additional_properties_keeps_fields() {
        use oas3::spec::Schema;
        let mut properties = BTreeMap::new();
        properties.insert(
            "id".to_string(),
            ObjectOrReference::Object(make_string_schema()),
        );

        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Object)),
            properties,
            required: vec!["id".to_string()],
            additional_properties: Some(Schema::Boolean(oas3::spec::BooleanSchema(true))),
            ..Default::default()
        };
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty =
            normalize_schema(&schema, Some("Extensible"), &mut visiting, &all_schemas).unwrap();
        assert!(
            matches!(&ty, IrType::Object(obj) if obj.fields.contains_key("id")),
            "defined properties must be preserved when additionalProperties is also set; got {ty:?}"
        );
    }

    #[test]
    fn test_inline_single_enum_produces_string_literal() {
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::String)),
            enum_values: vec![serde_json::Value::String("rtp_stream".to_string())],
            ..Default::default()
        };
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        // name=None → must NOT fall back to IrType::Enum { name: "Enum" }
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();
        assert!(
            matches!(&ty, IrType::StringLiteral(s) if s == "rtp_stream"),
            "single-value inline enum must produce StringLiteral(\"rtp_stream\"), got {ty:?}"
        );
    }

    #[test]
    fn test_inline_multi_enum_produces_union_of_string_literals() {
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::String)),
            enum_values: vec![
                serde_json::Value::String("mp4".to_string()),
                serde_json::Value::String("hls".to_string()),
            ],
            ..Default::default()
        };
        let mut visiting = HashSet::new();
        let all_schemas = BTreeMap::new();
        let ty = normalize_schema(&schema, None, &mut visiting, &all_schemas).unwrap();
        if let IrType::Union(variants) = ty {
            assert_eq!(variants.len(), 2);
            assert!(matches!(&variants[0], IrType::StringLiteral(s) if s == "mp4"));
            assert!(matches!(&variants[1], IrType::StringLiteral(s) if s == "hls"));
        } else {
            panic!("multi-value inline enum must produce Union of StringLiterals, got {ty:?}");
        }
    }

    #[test]
    fn test_normalize_array_with_items() {
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Array)),
            items: Some(Box::new(oas3::spec::Schema::Object(Box::new(
                ObjectOrReference::Object(make_string_schema()),
            )))),
            ..Default::default()
        };
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
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Array)),
            ..Default::default()
        };
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
        let schema = ObjectSchema {
            one_of: vec![
                ObjectOrReference::Object(make_string_schema()),
                ObjectOrReference::Object(ObjectSchema {
                    schema_type: Some(SchemaTypeSet::Single(SchemaType::Integer)),
                    ..Default::default()
                }),
            ],
            ..Default::default()
        };
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
        let schema = ObjectSchema {
            any_of: vec![
                ObjectOrReference::Object(make_string_schema()),
                ObjectOrReference::Object(ObjectSchema {
                    schema_type: Some(SchemaTypeSet::Single(SchemaType::Boolean)),
                    ..Default::default()
                }),
            ],
            ..Default::default()
        };
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
            ObjectOrReference::Object(ObjectSchema {
                schema_type: Some(SchemaTypeSet::Single(SchemaType::Integer)),
                ..Default::default()
            }),
        );
        let base = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Object)),
            properties: base_props,
            required: vec!["id".to_string()],
            ..Default::default()
        };

        let mut ext_props = BTreeMap::new();
        ext_props.insert(
            "role".to_string(),
            ObjectOrReference::Object(make_string_schema()),
        );
        let ext = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Object)),
            properties: ext_props,
            ..Default::default()
        };

        let schema = ObjectSchema {
            all_of: vec![
                ObjectOrReference::Object(base),
                ObjectOrReference::Object(ext),
            ],
            ..Default::default()
        };

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
                summary: None,
                description: None,
            },
        );
        let node_schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Object)),
            properties,
            ..Default::default()
        };

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
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Object)),
            additional_properties: Some(Schema::Object(Box::new(ObjectOrReference::Object(
                make_string_schema(),
            )))),
            ..Default::default()
        };

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
    fn test_operation_ref_resolves_to_actual_type_not_recursive() {
        // Regression: resolver previously pre-populated visited_schemas with all component
        // names. extract_operations cloned that set as the initial visiting state, so every
        // $ref to a component schema was immediately seen as "already visiting" and returned
        // IrType::Recursive instead of the actual type.
        use crate::pipeline::resolver::resolve;

        let yaml = include_str!("../../tests/fixtures/operation_refs.yaml");
        let spec: oas3::OpenApiV3Spec = serde_yaml::from_str(yaml).unwrap();
        let resolved = resolve(spec).unwrap();
        let api = normalize(resolved).unwrap();

        let op = api
            .operations
            .iter()
            .find(|o| o.id == "listPets")
            .expect("operation not found");

        let param = op.params.iter().find(|p| p.name == "status").unwrap();
        assert!(
            matches!(param.ty, IrType::String(_)),
            "operation param $ref must resolve to actual type, not Recursive; got {:?}",
            param.ty
        );

        let resp = op.responses.iter().find(|r| r.status == 200).unwrap();
        assert!(
            matches!(resp.ty, Some(IrType::Object(_))),
            "operation response $ref must resolve to actual type, not Recursive; got {:?}",
            resp.ty
        );
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
