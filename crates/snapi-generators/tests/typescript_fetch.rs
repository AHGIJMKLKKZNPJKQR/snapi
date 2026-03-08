use indexmap::IndexMap;
use snapi_core::generator::Generator;
use snapi_core::generator::TargetConfig;
use snapi_core::ir::api::{IrApi, IrServer};
use snapi_core::ir::operation::{
    HttpMethod, IrOperation, IrParam, IrRequestBody, IrResponse, ParamLocation,
};
use snapi_core::ir::types::{
    IrEnum, IrEnumVariant, IrField, IrIntConstraints, IrObject, IrStringConstraints, IrType,
};
use snapi_generators::typescript::fetch::FetchGenerator;

fn default_config() -> TargetConfig {
    TargetConfig {
        name: "test-sdk".to_string(),
        version: "0.1.0".to_string(),
        description: None,
        dir: std::path::PathBuf::from("sdks/typescript"),
        publish: None,
    }
}

fn empty_api(title: &str) -> IrApi {
    IrApi {
        title: title.to_string(),
        version: "1.0.0".to_string(),
        description: None,
        servers: vec![],
        schemas: IndexMap::new(),
        operations: vec![],
        auth_schemes: IndexMap::new(),
        webhooks: vec![],
    }
}

fn str_type() -> IrType {
    IrType::String(IrStringConstraints {
        min_length: None,
        max_length: None,
        pattern: None,
        format: None,
    })
}

fn int_type() -> IrType {
    IrType::Integer(IrIntConstraints {
        minimum: None,
        maximum: None,
        format: None,
    })
}

fn build_test_api() -> IrApi {
    let mut schemas = IndexMap::new();

    // User object
    let mut user_fields = IndexMap::new();
    user_fields.insert(
        "id".to_string(),
        IrField {
            ty: IrType::Integer(IrIntConstraints {
                minimum: None,
                maximum: None,
                format: None,
            }),
            required: true,
            description: Some("User ID".to_string()),
        },
    );
    user_fields.insert(
        "name".to_string(),
        IrField {
            ty: IrType::String(IrStringConstraints {
                min_length: None,
                max_length: None,
                pattern: None,
                format: None,
            }),
            required: true,
            description: Some("User's full name".to_string()),
        },
    );
    user_fields.insert(
        "email".to_string(),
        IrField {
            ty: IrType::Optional(Box::new(IrType::String(IrStringConstraints {
                min_length: None,
                max_length: None,
                pattern: None,
                format: Some("email".to_string()),
            }))),
            required: false,
            description: Some("Email address".to_string()),
        },
    );
    schemas.insert(
        "User".to_string(),
        IrType::Object(IrObject {
            name: Some("User".to_string()),
            fields: user_fields,
        }),
    );

    // Status enum
    schemas.insert(
        "UserStatus".to_string(),
        IrType::Enum(IrEnum {
            name: "UserStatus".to_string(),
            variants: vec![
                IrEnumVariant {
                    name: "Active".to_string(),
                    ty: IrType::String(IrStringConstraints {
                        min_length: None,
                        max_length: None,
                        pattern: None,
                        format: None,
                    }),
                },
                IrEnumVariant {
                    name: "Inactive".to_string(),
                    ty: IrType::String(IrStringConstraints {
                        min_length: None,
                        max_length: None,
                        pattern: None,
                        format: None,
                    }),
                },
            ],
            discriminator: None,
        }),
    );

    let operations = vec![
        IrOperation {
            id: "listUsers".to_string(),
            path: "/users".to_string(),
            method: HttpMethod::Get,
            summary: Some("List all users".to_string()),
            description: None,
            tags: vec!["users".to_string()],
            params: vec![],
            body: None,
            responses: vec![IrResponse {
                status: 200,
                content_type: Some("application/json".to_string()),
                ty: Some(IrType::Array {
                    items: Box::new(IrType::Object(IrObject {
                        name: Some("User".to_string()),
                        fields: IndexMap::new(),
                    })),
                    min: None,
                    max: None,
                }),
                description: Some("A list of users".to_string()),
            }],
            deprecated: false,
        },
        IrOperation {
            id: "getUser".to_string(),
            path: "/users/{id}".to_string(),
            method: HttpMethod::Get,
            summary: Some("Get a user by ID".to_string()),
            description: None,
            tags: vec!["users".to_string()],
            params: vec![IrParam {
                name: "id".to_string(),
                location: ParamLocation::Path,
                required: true,
                ty: IrType::Integer(IrIntConstraints {
                    minimum: None,
                    maximum: None,
                    format: None,
                }),
                description: None,
            }],
            body: None,
            responses: vec![IrResponse {
                status: 200,
                content_type: Some("application/json".to_string()),
                ty: Some(IrType::Object(IrObject {
                    name: Some("User".to_string()),
                    fields: IndexMap::new(),
                })),
                description: Some("A user".to_string()),
            }],
            deprecated: false,
        },
    ];

    IrApi {
        title: "Test API".to_string(),
        version: "1.0.0".to_string(),
        description: Some("A test API".to_string()),
        servers: vec![IrServer {
            url: "https://api.example.com".to_string(),
            description: None,
        }],
        schemas,
        operations,
        auth_schemes: IndexMap::new(),
        webhooks: vec![],
    }
}

#[test]
fn test_fetch_generator_produces_files() {
    let api = build_test_api();
    let config = TargetConfig {
        name: "test-sdk".to_string(),
        version: "0.1.0".to_string(),
        description: Some("Test SDK".to_string()),
        dir: std::path::PathBuf::from("sdks/typescript"),
        publish: None,
    };

    let tree = FetchGenerator.generate(&api, &config).unwrap();

    assert!(tree
        .files
        .contains_key(std::path::Path::new("src/models.ts")));
    assert!(tree
        .files
        .contains_key(std::path::Path::new("src/client.ts")));
    assert!(tree
        .files
        .contains_key(std::path::Path::new("src/index.ts")));
    assert!(tree
        .files
        .contains_key(std::path::Path::new("src/resources/users.ts")));
}

#[test]
fn test_models_snapshot() {
    let api = build_test_api();
    let config = TargetConfig {
        name: "test-sdk".to_string(),
        version: "0.1.0".to_string(),
        description: None,
        dir: std::path::PathBuf::from("sdks/typescript"),
        publish: None,
    };

    let tree = FetchGenerator.generate(&api, &config).unwrap();
    let models = match tree
        .files
        .get(std::path::Path::new("src/models.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };

    insta::assert_snapshot!("models_ts", models);
}

#[test]
fn test_users_resource_snapshot() {
    let api = build_test_api();
    let config = TargetConfig {
        name: "test-sdk".to_string(),
        version: "0.1.0".to_string(),
        description: None,
        dir: std::path::PathBuf::from("sdks/typescript"),
        publish: None,
    };

    let tree = FetchGenerator.generate(&api, &config).unwrap();
    let resource = match tree
        .files
        .get(std::path::Path::new("src/resources/users.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };

    insta::assert_snapshot!("users_resource_ts", resource);
}

// ---------------------------------------------------------------------------
// POST with request body
// ---------------------------------------------------------------------------

fn build_create_api() -> IrApi {
    let mut schemas = IndexMap::new();
    let mut fields = IndexMap::new();
    fields.insert(
        "name".to_string(),
        IrField {
            ty: str_type(),
            required: true,
            description: None,
        },
    );
    fields.insert(
        "email".to_string(),
        IrField {
            ty: str_type(),
            required: true,
            description: None,
        },
    );
    schemas.insert(
        "CreateUserInput".to_string(),
        IrType::Object(IrObject {
            name: Some("CreateUserInput".to_string()),
            fields: fields.clone(),
        }),
    );
    let mut user_fields = fields.clone();
    user_fields.insert(
        "id".to_string(),
        IrField {
            ty: int_type(),
            required: true,
            description: None,
        },
    );
    schemas.insert(
        "User".to_string(),
        IrType::Object(IrObject {
            name: Some("User".to_string()),
            fields: user_fields,
        }),
    );

    let mut api = empty_api("Create API");
    api.schemas = schemas;
    api.operations = vec![IrOperation {
        id: "createUser".to_string(),
        path: "/users".to_string(),
        method: HttpMethod::Post,
        summary: Some("Create a user".to_string()),
        description: None,
        tags: vec!["users".to_string()],
        params: vec![],
        body: Some(IrRequestBody {
            required: true,
            content_type: "application/json".to_string(),
            ty: IrType::Object(IrObject {
                name: Some("CreateUserInput".to_string()),
                fields: IndexMap::new(),
            }),
            description: None,
        }),
        responses: vec![IrResponse {
            status: 201,
            content_type: Some("application/json".to_string()),
            ty: Some(IrType::Object(IrObject {
                name: Some("User".to_string()),
                fields: IndexMap::new(),
            })),
            description: Some("Created user".to_string()),
        }],
        deprecated: false,
    }];
    api
}

#[test]
fn test_post_with_body_generates_body_param() {
    let api = build_create_api();
    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    let resource = match tree
        .files
        .get(std::path::Path::new("src/resources/users.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };
    // The generated method should include a `body` parameter and `JSON.stringify`
    assert!(
        resource.contains("body: CreateUserInput"),
        "missing body param: {}",
        resource
    );
    assert!(
        resource.contains("JSON.stringify(body)"),
        "missing JSON.stringify: {}",
        resource
    );
}

#[test]
fn test_post_with_body_snapshot() {
    let api = build_create_api();
    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    let resource = match tree
        .files
        .get(std::path::Path::new("src/resources/users.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };
    insta::assert_snapshot!("create_user_resource_ts", resource);
}

// ---------------------------------------------------------------------------
// Query params (pagination)
// ---------------------------------------------------------------------------

fn build_pagination_api() -> IrApi {
    let mut api = empty_api("Paginated API");
    api.operations = vec![IrOperation {
        id: "listItems".to_string(),
        path: "/items".to_string(),
        method: HttpMethod::Get,
        summary: Some("List items with pagination".to_string()),
        description: None,
        tags: vec!["items".to_string()],
        params: vec![
            IrParam {
                name: "page".to_string(),
                location: ParamLocation::Query,
                required: false,
                ty: int_type(),
                description: Some("Page number".to_string()),
            },
            IrParam {
                name: "limit".to_string(),
                location: ParamLocation::Query,
                required: false,
                ty: int_type(),
                description: Some("Items per page".to_string()),
            },
        ],
        body: None,
        responses: vec![IrResponse {
            status: 200,
            content_type: Some("application/json".to_string()),
            ty: Some(IrType::Array {
                items: Box::new(IrType::Any),
                min: None,
                max: None,
            }),
            description: Some("A page of items".to_string()),
        }],
        deprecated: false,
    }];
    api
}

#[test]
fn test_query_params_generate_query_object() {
    let api = build_pagination_api();
    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    let resource = match tree
        .files
        .get(std::path::Path::new("src/resources/items.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };
    assert!(
        resource.contains("query?:"),
        "missing query param: {}",
        resource
    );
    assert!(
        resource.contains("URLSearchParams"),
        "missing URLSearchParams: {}",
        resource
    );
}

#[test]
fn test_pagination_resource_snapshot() {
    let api = build_pagination_api();
    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    let resource = match tree
        .files
        .get(std::path::Path::new("src/resources/items.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };
    insta::assert_snapshot!("pagination_resource_ts", resource);
}

// ---------------------------------------------------------------------------
// Recursive/circular schema
// ---------------------------------------------------------------------------

fn build_recursive_api() -> IrApi {
    let mut schemas = IndexMap::new();
    let mut node_fields = IndexMap::new();
    node_fields.insert(
        "value".to_string(),
        IrField {
            ty: int_type(),
            required: true,
            description: None,
        },
    );
    node_fields.insert(
        "child".to_string(),
        IrField {
            ty: IrType::Optional(Box::new(IrType::Recursive("Node".to_string()))),
            required: false,
            description: None,
        },
    );
    schemas.insert(
        "Node".to_string(),
        IrType::Object(IrObject {
            name: Some("Node".to_string()),
            fields: node_fields,
        }),
    );

    let mut api = empty_api("Recursive API");
    api.schemas = schemas;
    api
}

#[test]
fn test_recursive_schema_generates_models() {
    let api = build_recursive_api();
    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    let models = match tree
        .files
        .get(std::path::Path::new("src/models.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };
    // The circular field should be typed as `Node | null`
    assert!(models.contains("Node"), "Node type missing: {}", models);
    assert!(models.contains("child"), "child field missing: {}", models);
}

#[test]
fn test_recursive_schema_snapshot() {
    let api = build_recursive_api();
    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    let models = match tree
        .files
        .get(std::path::Path::new("src/models.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };
    insta::assert_snapshot!("recursive_models_ts", models);
}

// ---------------------------------------------------------------------------
// Union type
// ---------------------------------------------------------------------------

#[test]
fn test_union_type_renders_pipe_separated() {
    let mut schemas = IndexMap::new();
    schemas.insert(
        "StringOrInt".to_string(),
        IrType::Union(vec![str_type(), int_type()]),
    );

    let mut api = empty_api("Union API");
    api.schemas = schemas;

    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    let models = match tree
        .files
        .get(std::path::Path::new("src/models.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };
    assert!(
        models.contains("string | number"),
        "expected union type: {}",
        models
    );
}

// ---------------------------------------------------------------------------
// Untagged operation → default resource
// ---------------------------------------------------------------------------

#[test]
fn test_untagged_operation_goes_to_default_resource() {
    let mut api = empty_api("Untagged API");
    api.operations = vec![IrOperation {
        id: "healthCheck".to_string(),
        path: "/health".to_string(),
        method: HttpMethod::Get,
        summary: None,
        description: None,
        tags: vec![], // no tags
        params: vec![],
        body: None,
        responses: vec![],
        deprecated: false,
    }];

    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    assert!(
        tree.files
            .contains_key(std::path::Path::new("src/resources/default.ts")),
        "expected default resource file"
    );
}

// ---------------------------------------------------------------------------
// Deprecated operation
// ---------------------------------------------------------------------------

#[test]
fn test_deprecated_operation_still_generates() {
    let mut api = empty_api("Deprecated API");
    api.operations = vec![IrOperation {
        id: "oldEndpoint".to_string(),
        path: "/old".to_string(),
        method: HttpMethod::Get,
        summary: Some("Old endpoint".to_string()),
        description: None,
        tags: vec!["legacy".to_string()],
        params: vec![],
        body: None,
        responses: vec![],
        deprecated: true,
    }];

    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    let resource = match tree
        .files
        .get(std::path::Path::new("src/resources/legacy.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };
    assert!(
        resource.contains("oldEndpoint"),
        "deprecated op missing: {}",
        resource
    );
}

// ---------------------------------------------------------------------------
// Multiple tags → multiple resource files
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_tags_produce_multiple_resource_files() {
    let mut api = empty_api("Multi-tag API");
    api.operations = vec![
        IrOperation {
            id: "listUsers".to_string(),
            path: "/users".to_string(),
            method: HttpMethod::Get,
            summary: None,
            description: None,
            tags: vec!["users".to_string()],
            params: vec![],
            body: None,
            responses: vec![],
            deprecated: false,
        },
        IrOperation {
            id: "listOrders".to_string(),
            path: "/orders".to_string(),
            method: HttpMethod::Get,
            summary: None,
            description: None,
            tags: vec!["orders".to_string()],
            params: vec![],
            body: None,
            responses: vec![],
            deprecated: false,
        },
    ];

    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    assert!(tree
        .files
        .contains_key(std::path::Path::new("src/resources/users.ts")));
    assert!(tree
        .files
        .contains_key(std::path::Path::new("src/resources/orders.ts")));
}

// ---------------------------------------------------------------------------
// client.ts content
// ---------------------------------------------------------------------------

#[test]
fn test_client_exposes_resource_fields() {
    let api = build_test_api();
    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    let client = match tree
        .files
        .get(std::path::Path::new("src/client.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };
    assert!(
        client.contains("UsersResource"),
        "missing UsersResource: {}",
        client
    );
    assert!(
        client.contains("readonly users:"),
        "missing users field: {}",
        client
    );
}

#[test]
fn test_client_snapshot() {
    let api = build_test_api();
    let tree = FetchGenerator.generate(&api, &default_config()).unwrap();
    let client = match tree
        .files
        .get(std::path::Path::new("src/client.ts"))
        .unwrap()
    {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text"),
    };
    insta::assert_snapshot!("client_ts", client);
}
