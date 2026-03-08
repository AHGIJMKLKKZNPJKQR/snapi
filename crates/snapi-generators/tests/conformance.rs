/// End-to-end conformance tests.
///
/// Each test loads a real OpenAPI 3.1 fixture from `tests/fixtures/`, runs the
/// full parse → resolve → normalize → generate pipeline, and asserts structural
/// properties of the resulting IR and file tree.
///
/// The `*_tsc` tests additionally write the generated SDK to a temporary
/// directory and invoke `tsc --noEmit` to verify the output compiles.
/// `tsc` must be installed and in PATH; the test fails if it is not found.
use std::path::{Path, PathBuf};
use std::process::Command;

use snapi_core::generator::{Generator, TargetConfig};
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
    }
}

fn make_tera() -> tera::Tera {
    let mut t = tera::Tera::default();
    t.add_raw_template(
        "typescript/package.json.tera",
        include_str!("../../snapi-generators/templates/typescript/package.json.tera"),
    )
    .unwrap();
    t.add_raw_template(
        "typescript/tsconfig.json.tera",
        include_str!("../../snapi-generators/templates/typescript/tsconfig.json.tera"),
    )
    .unwrap();
    t.add_raw_template(
        "typescript/README.md.tera",
        include_str!("../../snapi-generators/templates/typescript/README.md.tera"),
    )
    .unwrap();
    t
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

/// Resolves the `tsc` binary from the workspace-local `node_modules/.bin/tsc`.
fn tsc_bin() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("node_modules")
        .join(".bin")
        .join("tsc")
}

/// Write the FileTree to a temp dir and run `tsc --noEmit`.
/// Panics if `tsc` is not found or if compilation fails.
fn tsc_check(tree: &snapi_core::file_tree::FileTree) {
    let dir = tempfile::tempdir().expect("temp dir");
    let tera = make_tera();
    tree.write_to_disk(dir.path(), &tera)
        .expect("write to disk");

    let tsc = tsc_bin();
    let out = Command::new(&tsc)
        .arg("--noEmit")
        .current_dir(dir.path())
        .output()
        .unwrap_or_else(|e| panic!("failed to invoke tsc at {}: {e}", tsc.display()));

    if !out.status.success() {
        panic!(
            "tsc --noEmit failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
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

#[test]
fn petstore_tsc() {
    let ir = load_ir("petstore.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("petstore-sdk"))
        .unwrap();
    tsc_check(&tree);
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

#[test]
fn crud_with_auth_tsc() {
    let ir = load_ir("crud_with_auth.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("todo-sdk"))
        .unwrap();
    tsc_check(&tree);
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

#[test]
fn discriminated_union_tsc() {
    let ir = load_ir("discriminated_union.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("events-sdk"))
        .unwrap();
    tsc_check(&tree);
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

#[test]
fn circular_refs_tsc() {
    let ir = load_ir("circular_refs.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("tree-sdk"))
        .unwrap();
    tsc_check(&tree);
}
