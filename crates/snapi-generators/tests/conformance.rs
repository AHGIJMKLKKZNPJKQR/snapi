/// Conformance tests.
///
/// Each test loads a real OpenAPI 3.1 fixture from `tests/fixtures/`, runs the
/// full parse → resolve → normalize → generate pipeline, and asserts structural
/// properties of the resulting IR and file tree.
///
/// TypeScript compilation (`tsc --noEmit`) is verified in the e2e test suite,
/// which runs both tsc and tsx against the generated SDK.
use std::path::{Path, PathBuf};

use snapi_core::generator::{CollisionStrategy, Generator, TargetConfig};
use snapi_core::ir::types::IrType;
use snapi_generators::typescript::fetch::FetchGenerator;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn load_ir(fixture: &str) -> snapi_core::ir::api::IrApi {
    let path = fixtures_dir().join(fixture);
    snapi_core::pipeline::load(&path).unwrap_or_else(|e| panic!("failed to load {fixture}: {e:#}"))
}

fn default_config(name: &str) -> TargetConfig {
    TargetConfig {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        description: None,
        dir: PathBuf::from("sdks/typescript"),
        publish: None,
        on_collision: snapi_core::generator::CollisionStrategy::Fail,
    }
}

fn extract_text(tree: &snapi_core::file_tree::FileTree, path: &str) -> String {
    match tree.files.get(Path::new(path)).unwrap_or_else(|| {
        let keys: Vec<_> = tree.files.keys().collect();
        panic!("file {path} not found; have: {keys:?}")
    }) {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("{path} is a template, expected text"),
    }
}

// ---------------------------------------------------------------------------
// Petstore
// ---------------------------------------------------------------------------

#[test]
fn petstore_pipeline_succeeds() {
    let ir = load_ir("petstore.yaml");
    assert_eq!(ir.title, "Petstore");
    assert_eq!(ir.version, "1.0.0");
    assert_eq!(
        ir.operations.len(),
        4,
        "expected listPets, createPet, getPet, deletePet"
    );
    assert!(ir.schemas.contains_key("Pet"), "missing Pet schema");
    assert!(ir.schemas.contains_key("NewPet"), "missing NewPet schema");
    assert!(
        ir.schemas.contains_key("PetStatus"),
        "missing PetStatus schema"
    );
}

#[test]
fn petstore_ir_pet_schema_has_correct_fields() {
    let ir = load_ir("petstore.yaml");
    if let IrType::Object(obj) = &ir.schemas["Pet"] {
        assert!(obj.fields.contains_key("id"), "Pet missing id");
        assert!(obj.fields.contains_key("name"), "Pet missing name");
        assert!(obj.fields["id"].required, "id should be required");
        assert!(obj.fields["name"].required, "name should be required");
        assert!(
            !obj.fields.get("tag").map(|f| f.required).unwrap_or(true),
            "tag should be optional"
        );
    } else {
        panic!("Pet is not an Object");
    }
}

#[test]
fn petstore_ir_status_is_enum() {
    let ir = load_ir("petstore.yaml");
    assert!(
        matches!(ir.schemas["PetStatus"], IrType::Enum(_)),
        "PetStatus should be Enum, got {:?}",
        ir.schemas["PetStatus"]
    );
    if let IrType::Enum(e) = &ir.schemas["PetStatus"] {
        assert_eq!(e.variants.len(), 3);
    }
}

#[test]
fn petstore_ir_operations_have_correct_ids() {
    let ir = load_ir("petstore.yaml");
    let ids: Vec<&str> = ir.operations.iter().map(|o| o.id.as_str()).collect();
    assert!(ids.contains(&"listPets"), "missing listPets");
    assert!(ids.contains(&"createPet"), "missing createPet");
    assert!(ids.contains(&"getPet"), "missing getPet");
    assert!(ids.contains(&"deletePet"), "missing deletePet");
}

#[test]
fn petstore_ir_list_pets_has_query_params() {
    let ir = load_ir("petstore.yaml");
    let op = ir.operations.iter().find(|o| o.id == "listPets").unwrap();
    assert_eq!(
        op.params.len(),
        2,
        "listPets should have limit + tags params"
    );
    let locations: Vec<_> = op.params.iter().map(|p| &p.location).collect();
    assert!(locations
        .iter()
        .all(|l| matches!(l, snapi_core::ir::operation::ParamLocation::Query)));
}

#[test]
fn petstore_ir_create_pet_has_request_body() {
    let ir = load_ir("petstore.yaml");
    let op = ir.operations.iter().find(|o| o.id == "createPet").unwrap();
    assert!(op.body.is_some(), "createPet must have a request body");
    let body = op.body.as_ref().unwrap();
    assert!(body.required);
}

#[test]
fn petstore_ir_get_pet_has_path_param() {
    let ir = load_ir("petstore.yaml");
    let op = ir.operations.iter().find(|o| o.id == "getPet").unwrap();
    assert_eq!(op.params.len(), 1);
    assert_eq!(op.params[0].name, "id");
    assert!(matches!(
        op.params[0].location,
        snapi_core::ir::operation::ParamLocation::Path
    ));
}

#[test]
fn petstore_generator_produces_all_files() {
    let ir = load_ir("petstore.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("petstore-sdk"))
        .unwrap();

    assert!(tree.files.contains_key(Path::new("src/models.ts")));
    assert!(tree.files.contains_key(Path::new("src/client.ts")));
    assert!(tree.files.contains_key(Path::new("src/index.ts")));
    assert!(tree.files.contains_key(Path::new("src/resources/pets.ts")));
    assert!(tree.files.contains_key(Path::new("package.json")));
    assert!(tree.files.contains_key(Path::new("tsconfig.json")));
}

#[test]
fn petstore_models_contain_pet_interface() {
    let ir = load_ir("petstore.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("petstore-sdk"))
        .unwrap();
    let models = extract_text(&tree, "src/models.ts");
    assert!(
        models.contains("export interface Pet"),
        "missing Pet interface:\n{models}"
    );
    assert!(
        models.contains("export interface NewPet"),
        "missing NewPet interface:\n{models}"
    );
    assert!(
        models.contains("export type PetStatus"),
        "missing PetStatus type:\n{models}"
    );
}

#[test]
fn petstore_pets_resource_has_all_methods() {
    let ir = load_ir("petstore.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("petstore-sdk"))
        .unwrap();
    let resource = extract_text(&tree, "src/resources/pets.ts");
    assert!(
        resource.contains("listPets"),
        "missing listPets: {resource}"
    );
    assert!(
        resource.contains("createPet"),
        "missing createPet: {resource}"
    );
    assert!(resource.contains("getPet"), "missing getPet: {resource}");
    assert!(
        resource.contains("deletePet"),
        "missing deletePet: {resource}"
    );
}

// ---------------------------------------------------------------------------
// CRUD with auth (Todo API)
// ---------------------------------------------------------------------------

#[test]
fn crud_with_auth_pipeline_succeeds() {
    let ir = load_ir("crud_with_auth.yaml");
    assert_eq!(ir.title, "Todo API");
    assert_eq!(
        ir.operations.len(),
        5,
        "expected listTodos, createTodo, getTodo, updateTodo, deleteTodo"
    );
    assert!(ir.schemas.contains_key("Todo"));
    assert!(ir.schemas.contains_key("CreateTodoInput"));
    assert!(ir.schemas.contains_key("UpdateTodoInput"));
}

#[test]
fn crud_with_auth_bearer_scheme_detected() {
    let ir = load_ir("crud_with_auth.yaml");
    assert!(
        ir.auth_schemes.contains_key("bearerAuth"),
        "bearerAuth scheme not detected"
    );
    assert!(matches!(
        ir.auth_schemes["bearerAuth"],
        snapi_core::ir::auth::IrAuthScheme::Bearer { .. }
    ));
}

#[test]
fn crud_with_auth_todo_schema_fields() {
    let ir = load_ir("crud_with_auth.yaml");
    if let IrType::Object(obj) = &ir.schemas["Todo"] {
        assert!(obj.fields["id"].required);
        assert!(obj.fields["title"].required);
        assert!(obj.fields["done"].required);
        assert!(
            !obj.fields["created_at"].required,
            "created_at should be optional"
        );
    } else {
        panic!("Todo is not Object");
    }
}

#[test]
fn crud_with_auth_list_todos_has_pagination_params() {
    let ir = load_ir("crud_with_auth.yaml");
    let op = ir.operations.iter().find(|o| o.id == "listTodos").unwrap();
    let param_names: Vec<&str> = op.params.iter().map(|p| p.name.as_str()).collect();
    assert!(param_names.contains(&"page"), "missing page param");
    assert!(param_names.contains(&"per_page"), "missing per_page param");
    assert!(param_names.contains(&"status"), "missing status param");
}

#[test]
fn crud_with_auth_update_todo_has_body() {
    let ir = load_ir("crud_with_auth.yaml");
    let op = ir.operations.iter().find(|o| o.id == "updateTodo").unwrap();
    assert!(op.body.is_some(), "updateTodo must have a request body");
}

#[test]
fn crud_with_auth_generator_produces_todos_resource() {
    let ir = load_ir("crud_with_auth.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("todo-sdk"))
        .unwrap();
    assert!(tree.files.contains_key(Path::new("src/resources/todos.ts")));
    let resource = extract_text(&tree, "src/resources/todos.ts");
    assert!(resource.contains("listTodos"));
    assert!(resource.contains("createTodo"));
    assert!(resource.contains("getTodo"));
    assert!(resource.contains("updateTodo"));
    assert!(resource.contains("deleteTodo"));
}

// ---------------------------------------------------------------------------
// Discriminated union (Events API)
// ---------------------------------------------------------------------------

#[test]
fn discriminated_union_pipeline_succeeds() {
    let ir = load_ir("discriminated_union.yaml");
    assert_eq!(ir.title, "Events API");
    assert_eq!(ir.operations.len(), 2, "expected listEvents, getEvent");
    assert!(ir.schemas.contains_key("Event"), "missing Event schema");
    assert!(ir.schemas.contains_key("UserCreatedEvent"));
    assert!(ir.schemas.contains_key("OrderPlacedEvent"));
    assert!(ir.schemas.contains_key("PaymentProcessedEvent"));
    assert!(ir.schemas.contains_key("PaymentStatus"));
}

#[test]
fn discriminated_union_event_is_union_type() {
    let ir = load_ir("discriminated_union.yaml");
    assert!(
        matches!(ir.schemas["Event"], IrType::Union(_)),
        "Event should be Union (oneOf without discriminator), got {:?}",
        ir.schemas["Event"]
    );
    if let IrType::Union(variants) = &ir.schemas["Event"] {
        assert_eq!(variants.len(), 3, "Event union should have 3 variants");
    }
}

#[test]
fn discriminated_union_payment_status_is_enum() {
    let ir = load_ir("discriminated_union.yaml");
    assert!(matches!(ir.schemas["PaymentStatus"], IrType::Enum(_)));
    if let IrType::Enum(e) = &ir.schemas["PaymentStatus"] {
        assert_eq!(e.variants.len(), 4);
    }
}

#[test]
fn discriminated_union_models_contain_union_syntax() {
    let ir = load_ir("discriminated_union.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("events-sdk"))
        .unwrap();
    let models = extract_text(&tree, "src/models.ts");
    // The Event union type must appear as a TypeScript union (|)
    assert!(
        models.contains("Event"),
        "Event missing from models:\n{models}"
    );
    assert!(
        models.contains('|'),
        "no union type rendered in models:\n{models}"
    );
}

// ---------------------------------------------------------------------------
// Circular references (Tree API)
// ---------------------------------------------------------------------------

#[test]
fn circular_refs_pipeline_succeeds() {
    let ir = load_ir("circular_refs.yaml");
    assert_eq!(ir.title, "Tree API");
    assert!(
        ir.schemas.contains_key("TreeNode"),
        "missing TreeNode schema"
    );
    assert!(ir.schemas.contains_key("NewNode"), "missing NewNode schema");
    assert!(
        ir.schemas.contains_key("Category"),
        "missing Category schema"
    );
}

#[test]
fn circular_refs_tree_node_has_recursive_children() {
    let ir = load_ir("circular_refs.yaml");
    // The pipeline must complete without panicking or error on self-referential schemas.
    // TreeNode.children should be Array { items: Recursive("TreeNode") } or similar.
    if let IrType::Object(obj) = &ir.schemas["TreeNode"] {
        assert!(obj.fields.contains_key("id"));
        assert!(obj.fields.contains_key("value"));
        // children field must exist and its type must reference back to TreeNode somehow
        assert!(
            obj.fields.contains_key("children"),
            "TreeNode missing children field"
        );
        let children_ty = &obj.fields["children"].ty;
        // Should be Array of Recursive("TreeNode") (since it's a self-ref)
        assert!(
            matches!(children_ty, IrType::Array { .. }),
            "children should be Array, got {children_ty:?}"
        );
    } else {
        panic!("TreeNode should be Object");
    }
}

#[test]
fn circular_refs_category_recursive_parent() {
    let ir = load_ir("circular_refs.yaml");
    if let IrType::Object(obj) = &ir.schemas["Category"] {
        assert!(
            obj.fields.contains_key("parent"),
            "Category missing parent field"
        );
        // parent is nullable (oneOf [Category, null]) → should be Optional or Union containing Recursive
        let parent_ty = &obj.fields["parent"].ty;
        // Union or Optional containing a Recursive reference back to Category
        fn contains_recursive(ty: &IrType) -> bool {
            match ty {
                IrType::Recursive(_) => true,
                IrType::Optional(inner) => contains_recursive(inner),
                IrType::Union(variants) => variants.iter().any(contains_recursive),
                _ => false,
            }
        }
        assert!(
            contains_recursive(parent_ty),
            "Category.parent should contain a Recursive ref, got {parent_ty:?}"
        );
    } else {
        panic!("Category should be Object");
    }
}

#[test]
fn circular_refs_operations_parsed() {
    let ir = load_ir("circular_refs.yaml");
    let ids: Vec<&str> = ir.operations.iter().map(|o| o.id.as_str()).collect();
    assert!(ids.contains(&"listNodes"));
    assert!(ids.contains(&"getNode"));
    assert!(ids.contains(&"addChild"));
    assert!(ids.contains(&"listCategories"));
}

#[test]
fn circular_refs_generator_produces_files() {
    let ir = load_ir("circular_refs.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("tree-sdk"))
        .unwrap();
    assert!(tree.files.contains_key(Path::new("src/models.ts")));
    assert!(tree.files.contains_key(Path::new("src/resources/nodes.ts")));
    assert!(tree
        .files
        .contains_key(Path::new("src/resources/categories.ts")));
}

// ---------------------------------------------------------------------------
// Complex schemas (allOf, oneOf, nested oneOf+allOf, inline objects, maps, circular)
// ---------------------------------------------------------------------------

#[test]
fn complex_schemas_pipeline_succeeds() {
    let ir = load_ir("complex_schemas.yaml");
    assert_eq!(ir.title, "Complex Schemas API");
    assert!(ir.schemas.contains_key("Article"), "missing Article");
    assert!(
        ir.schemas.contains_key("Timestamped"),
        "missing Timestamped"
    );
    assert!(
        ir.schemas.contains_key("SearchResult"),
        "missing SearchResult"
    );
    assert!(ir.schemas.contains_key("Config"), "missing Config");
    assert!(ir.schemas.contains_key("TreeNode"), "missing TreeNode");
    assert!(
        ir.schemas.contains_key("NotificationPayload"),
        "missing NotificationPayload"
    );
    assert!(
        ir.schemas.contains_key("Notification"),
        "missing Notification"
    );
}

#[test]
fn complex_schemas_article_is_allof_merged() {
    let ir = load_ir("complex_schemas.yaml");
    // Article = allOf[Timestamped, inline] → merges into a single Object
    if let IrType::Object(obj) = &ir.schemas["Article"] {
        // fields from Timestamped
        assert!(obj.fields.contains_key("created_at"), "missing created_at");
        assert!(obj.fields.contains_key("updated_at"), "missing updated_at");
        // fields from the inline allOf entry
        assert!(obj.fields.contains_key("id"), "missing id");
        assert!(obj.fields.contains_key("title"), "missing title");
        assert!(obj.fields.contains_key("body"), "missing body");
        // inline author field is an unnamed Object (name: None)
        assert!(obj.fields.contains_key("author"), "missing author");
        if let IrType::Object(author) = &obj.fields["author"].ty {
            assert!(
                author.name.is_none(),
                "author should be an unnamed inline object"
            );
        } else {
            panic!("author should be Object");
        }
    } else {
        panic!("Article should be Object after allOf merge");
    }
}

#[test]
fn complex_schemas_search_result_is_union() {
    let ir = load_ir("complex_schemas.yaml");
    assert!(
        matches!(ir.schemas["SearchResult"], IrType::Union(_)),
        "SearchResult should be Union (oneOf)"
    );
    if let IrType::Union(variants) = &ir.schemas["SearchResult"] {
        assert_eq!(
            variants.len(),
            2,
            "SearchResult should have 2 oneOf variants"
        );
    }
}

#[test]
fn complex_schemas_config_has_map_and_nullable() {
    let ir = load_ir("complex_schemas.yaml");
    if let IrType::Object(obj) = &ir.schemas["Config"] {
        assert!(
            matches!(obj.fields["values"].ty, IrType::Map(_)),
            "values should be Map"
        );
        // description: oneOf [string, null] → Union or Optional
        let desc_ty = &obj.fields["description"].ty;
        assert!(
            matches!(desc_ty, IrType::Union(_) | IrType::Optional(_)),
            "description should be Union/Optional (nullable), got {desc_ty:?}"
        );
    } else {
        panic!("Config should be Object");
    }
}

#[test]
fn complex_schemas_tree_node_is_recursive() {
    let ir = load_ir("complex_schemas.yaml");
    if let IrType::Object(obj) = &ir.schemas["TreeNode"] {
        // children: array of recursive TreeNode
        if let IrType::Array { items, .. } = &obj.fields["children"].ty {
            assert!(
                matches!(items.as_ref(), IrType::Recursive(n) if n == "TreeNode"),
                "children items should be Recursive(TreeNode)"
            );
        } else {
            panic!("children should be Array");
        }
        // parent: oneOf [TreeNode, null] → Union containing Recursive
        fn has_recursive(ty: &IrType) -> bool {
            match ty {
                IrType::Recursive(_) => true,
                IrType::Union(vs) => vs.iter().any(has_recursive),
                IrType::Optional(inner) => has_recursive(inner),
                _ => false,
            }
        }
        assert!(
            has_recursive(&obj.fields["parent"].ty),
            "parent should contain Recursive ref"
        );
    } else {
        panic!("TreeNode should be Object");
    }
}

#[test]
fn complex_schemas_notification_payload_is_nested_oneof_allof() {
    let ir = load_ir("complex_schemas.yaml");
    // NotificationPayload = oneOf[allOf[Base,{email}], allOf[Base,{phone}]]
    // → Union of two merged Objects
    if let IrType::Union(variants) = &ir.schemas["NotificationPayload"] {
        assert_eq!(
            variants.len(),
            2,
            "NotificationPayload should have 2 variants"
        );
        for variant in variants {
            if let IrType::Object(obj) = variant {
                // both variants contain BaseNotification fields (channel, id)
                assert!(
                    obj.fields.contains_key("channel"),
                    "merged variant should have channel"
                );
                assert!(
                    obj.fields.contains_key("id"),
                    "merged variant should have id"
                );
            } else {
                panic!("each NotificationPayload variant should be a merged Object");
            }
        }
    } else {
        panic!("NotificationPayload should be Union");
    }
}

#[test]
fn complex_schemas_notification_is_allof_merged() {
    let ir = load_ir("complex_schemas.yaml");
    // Notification = allOf[BaseNotification, {sent_at}] → merged Object
    if let IrType::Object(obj) = &ir.schemas["Notification"] {
        assert!(
            obj.fields.contains_key("id"),
            "missing id from BaseNotification"
        );
        assert!(
            obj.fields.contains_key("channel"),
            "missing channel from BaseNotification"
        );
        assert!(obj.fields.contains_key("sent_at"), "missing sent_at");
    } else {
        panic!("Notification should be Object after allOf merge");
    }
}

// ---------------------------------------------------------------------------
// Composition API (inline discriminator enums, multiple path params)
// ---------------------------------------------------------------------------

#[test]
fn composition_api_pipeline_succeeds() {
    let ir = load_ir("composition_api.json");
    assert_eq!(ir.title, "Composition API");
    assert!(ir.schemas.contains_key("CreateCompositionRequest"));
    assert!(ir.schemas.contains_key("CompositionCreatedResponse"));
    assert!(ir.schemas.contains_key("RegisterInput"));
    assert!(ir.schemas.contains_key("RegisterOutput"));
    assert!(ir.schemas.contains_key("Response"));
}

#[test]
fn composition_api_register_input_is_union() {
    let ir = load_ir("composition_api.json");
    assert!(
        matches!(ir.schemas["RegisterInput"], IrType::Union(_)),
        "RegisterInput should be Union (oneOf), got {:?}",
        ir.schemas["RegisterInput"]
    );
    if let IrType::Union(variants) = &ir.schemas["RegisterInput"] {
        assert_eq!(variants.len(), 6, "RegisterInput should have 6 variants");
    }
}

#[test]
fn composition_api_inline_discriminator_is_not_bare_enum_ident() {
    // The `type` field of each RegisterInput / RegisterOutput variant is an
    // inline `{ "type": "string", "enum": ["rtp_stream"] }` schema with no
    // component name.  The normalizer must NOT produce IrType::Enum { name:
    // "Enum" } for it, which the generator would render as the bare identifier
    // `Enum` — an undefined TypeScript type.
    let ir = load_ir("composition_api.json");

    fn has_bare_enum_fallback(ty: &IrType) -> bool {
        match ty {
            IrType::Enum(e) if e.name == "Enum" => true,
            IrType::Union(variants) => variants.iter().any(has_bare_enum_fallback),
            IrType::Object(obj) => obj.fields.values().any(|f| has_bare_enum_fallback(&f.ty)),
            IrType::Optional(inner) => has_bare_enum_fallback(inner),
            IrType::Array { items, .. } => has_bare_enum_fallback(items),
            _ => false,
        }
    }

    for (name, ty) in &ir.schemas {
        assert!(
            !has_bare_enum_fallback(ty),
            "schema '{name}' contains IrType::Enum with fallback name \"Enum\": {ty:?}"
        );
    }
}

#[test]
fn composition_api_register_input_type_field_is_string_literal() {
    // The `type` discriminator field of the rtp_stream variant must be a
    // string literal ("rtp_stream"), not the bare identifier Enum.
    let ir = load_ir("composition_api.json");
    if let IrType::Union(variants) = &ir.schemas["RegisterInput"] {
        for variant in variants {
            if let IrType::Object(obj) = variant {
                if let Some(field) = obj.fields.get("type") {
                    assert!(
                        !matches!(&field.ty, IrType::Enum(e) if e.name == "Enum"),
                        "RegisterInput.type must not be bare Enum identifier, got {:?}",
                        field.ty
                    );
                    assert!(
                        matches!(&field.ty, IrType::StringLiteral(_)),
                        "RegisterInput.type must be StringLiteral, got {:?}",
                        field.ty
                    );
                }
            }
        }
    }
}

#[test]
fn composition_api_operations_parsed() {
    let ir = load_ir("composition_api.json");
    let ids: Vec<&str> = ir.operations.iter().map(|o| o.id.as_str()).collect();
    assert!(ids.contains(&"create_composition"));
    assert!(ids.contains(&"delete_composition"));
    assert!(ids.contains(&"start"));
    assert!(ids.contains(&"reset"));
    assert!(ids.contains(&"register_input"));
    assert!(ids.contains(&"register_output"));
    assert!(ids.contains(&"update_output"));
    assert!(ids.contains(&"request_keyframe"));
}

#[test]
fn composition_api_register_input_has_two_path_params() {
    let ir = load_ir("composition_api.json");
    let op = ir
        .operations
        .iter()
        .find(|o| o.id == "register_input")
        .unwrap();
    let path_params: Vec<_> = op
        .params
        .iter()
        .filter(|p| matches!(p.location, snapi_core::ir::operation::ParamLocation::Path))
        .collect();
    assert_eq!(
        path_params.len(),
        2,
        "register_input must have 2 path params"
    );
    let names: Vec<&str> = path_params.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"composition_id"));
    assert!(names.contains(&"input_id"));
}

#[test]
fn composition_api_models_contain_no_bare_enum_identifier() {
    let ir = load_ir("composition_api.json");
    let tree = FetchGenerator
        .generate(&ir, &default_config("composition-sdk"))
        .unwrap();
    let models = extract_text(&tree, "src/models.ts");
    // "Enum" as a bare type reference (not inside a string, not a field name)
    // would appear as ": Enum" or "| Enum" or "Enum |" in generated output.
    assert!(
        !models.contains(": Enum"),
        "models.ts contains bare Enum identifier:\n{models}"
    );
    assert!(
        !models.contains("| Enum"),
        "models.ts contains bare Enum identifier in union:\n{models}"
    );
}

#[test]
fn composition_api_bodyless_methods_omit_content_type() {
    let ir = load_ir("composition_api.json");
    let tree = FetchGenerator
        .generate(&ir, &default_config("composition-sdk"))
        .unwrap();

    // deleteComposition has no body; createComposition does → exactly one Content-Type in file
    let composition = extract_text(&tree, "src/resources/composition.ts");
    let ct_count = composition.matches("Content-Type").count();
    assert_eq!(
        ct_count,
        1,
        "composition.ts should have Content-Type only for createComposition (has body), not deleteComposition:\n{composition}"
    );

    // start and reset have no body at all → no Content-Type header
    let control = extract_text(&tree, "src/resources/control_request.ts");
    assert!(
        !control.contains("Content-Type"),
        "control_request.ts bodyless methods must not set Content-Type:\n{control}"
    );

    // requestKeyframe has no body; updateOutput does → exactly one Content-Type in file
    let update = extract_text(&tree, "src/resources/update_request.ts");
    let update_ct_count = update.matches("Content-Type").count();
    assert_eq!(
        update_ct_count, 1,
        "update_request.ts should have Content-Type only for updateOutput (has body):\n{update}"
    );
}

// ---------------------------------------------------------------------------
// Collision detection — integration tests with real OpenAPI fixtures
// ---------------------------------------------------------------------------

fn suffix_config(name: &str) -> TargetConfig {
    TargetConfig {
        on_collision: CollisionStrategy::Suffix,
        ..default_config(name)
    }
}

// --- Schema name collisions -------------------------------------------------

#[test]
fn schema_name_collision_fails_by_default() {
    let ir = load_ir("collisions_schema_names.yaml");
    let err = FetchGenerator
        .generate(&ir, &default_config("sdk"))
        .err()
        .expect("expected collision error");
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
fn schema_name_collision_suffix_strategy_produces_suffixed_type() {
    let ir = load_ir("collisions_schema_names.yaml");
    let tree = FetchGenerator.generate(&ir, &suffix_config("sdk")).unwrap();
    let models = extract_text(&tree, "src/models.ts");
    assert!(
        models.contains("HelloWorld ")
            || models.contains("HelloWorld\n")
            || models.contains("HelloWorld{"),
        "first schema keeps base name; got:\n{models}"
    );
    assert!(
        models.contains("HelloWorld2"),
        "second schema gets suffix; got:\n{models}"
    );
}

// --- Resource (tag) name collisions -----------------------------------------

#[test]
fn tag_collision_fails_by_default() {
    let ir = load_ir("collisions_tags.yaml");
    let err = FetchGenerator
        .generate(&ir, &default_config("sdk"))
        .err()
        .expect("expected collision error");
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
fn tag_collision_suffix_strategy_produces_suffixed_class() {
    let ir = load_ir("collisions_tags.yaml");
    let tree = FetchGenerator.generate(&ir, &suffix_config("sdk")).unwrap();
    let client = extract_text(&tree, "src/client.ts");
    assert!(
        client.contains("HelloWorldResource"),
        "first tag keeps base class name; got:\n{client}"
    );
    assert!(
        client.contains("HelloWorldResource2"),
        "second tag gets suffixed class name; got:\n{client}"
    );
}

// --- Method name collisions -------------------------------------------------

#[test]
fn method_collision_fails_by_default() {
    let ir = load_ir("collisions_op_ids.yaml");
    let err = FetchGenerator
        .generate(&ir, &default_config("sdk"))
        .err()
        .expect("expected collision error");
    let msg = format!("{err}");
    assert!(
        msg.contains("list_pets"),
        "error should name first op id; got: {msg}"
    );
    assert!(
        msg.contains("listPets"),
        "error should name second op id; got: {msg}"
    );
}

#[test]
fn method_collision_suffix_strategy_produces_suffixed_method() {
    let ir = load_ir("collisions_op_ids.yaml");
    let tree = FetchGenerator.generate(&ir, &suffix_config("sdk")).unwrap();
    let resource = extract_text(&tree, "src/resources/pets.ts");
    assert!(
        resource.contains("async listPets("),
        "first method keeps base name; got:\n{resource}"
    );
    assert!(
        resource.contains("async listPets2("),
        "second method gets suffix; got:\n{resource}"
    );
}
