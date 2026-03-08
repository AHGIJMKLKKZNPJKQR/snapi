/// End-to-end runtime tests.
///
/// Each test generates a TypeScript SDK, snapshots key generated files via
/// `insta`, and then runs a TypeScript test file against the SDK using `tsx`,
/// verifying that the generated client makes correct HTTP requests at runtime.
///
/// `tsx` must be installed: `npm install` from the workspace root.
use std::path::{Path, PathBuf};
use std::process::Command;

use snapi_core::generator::{Generator, TargetConfig};
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
    match tree.files.get(Path::new(path)).unwrap() {
        snapi_core::file_tree::FileContent::Text(s) => s.clone(),
        _ => panic!("expected text file at {path}"),
    }
}

fn bin(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("node_modules")
        .join(".bin")
        .join(name)
}

/// Write the SDK to a temp dir, verify it compiles with `tsc --noEmit`, then
/// run `test_src` with `tsx` to validate runtime behaviour.
fn run_tsx_test(tree: &snapi_core::file_tree::FileTree, test_src: &str) {
    let dir = tempfile::tempdir().expect("temp dir");
    let tera = make_tera();
    tree.write_to_disk(dir.path(), &tera)
        .expect("write to disk");

    // TypeScript compilation check
    let tsc = bin("tsc");
    let tsc_out = Command::new(&tsc)
        .arg("--noEmit")
        .current_dir(dir.path())
        .output()
        .unwrap_or_else(|e| panic!("failed to invoke tsc at {}: {e}", tsc.display()));
    if !tsc_out.status.success() {
        panic!(
            "tsc --noEmit failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&tsc_out.stdout),
            String::from_utf8_lossy(&tsc_out.stderr),
        );
    }

    // Runtime behaviour check
    std::fs::write(dir.path().join("test.ts"), test_src).expect("write test.ts");
    let tsx = bin("tsx");
    let out = Command::new(&tsx)
        .arg("test.ts")
        .current_dir(dir.path())
        .output()
        .unwrap_or_else(|e| panic!("failed to invoke tsx at {}: {e}", tsx.display()));
    if !out.status.success() {
        panic!(
            "e2e test failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn petstore() {
    let ir = load_ir("petstore.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("petstore-sdk"))
        .unwrap();

    insta::assert_snapshot!("petstore_models_ts", extract_text(&tree, "src/models.ts"));
    insta::assert_snapshot!("petstore_client_ts", extract_text(&tree, "src/client.ts"));
    insta::assert_snapshot!(
        "petstore_pets_resource_ts",
        extract_text(&tree, "src/resources/pets.ts")
    );

    run_tsx_test(&tree, include_str!("e2e_ts/petstore_test.ts"));
}

#[test]
fn crud_with_auth() {
    let ir = load_ir("crud_with_auth.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("todo-sdk"))
        .unwrap();

    insta::assert_snapshot!(
        "crud_with_auth_models_ts",
        extract_text(&tree, "src/models.ts")
    );
    insta::assert_snapshot!(
        "crud_with_auth_todos_resource_ts",
        extract_text(&tree, "src/resources/todos.ts")
    );

    run_tsx_test(&tree, include_str!("e2e_ts/crud_with_auth_test.ts"));
}

#[test]
fn complex_schemas() {
    let ir = load_ir("complex_schemas.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("complex-sdk"))
        .unwrap();

    insta::assert_snapshot!(
        "complex_schemas_models_ts",
        extract_text(&tree, "src/models.ts")
    );
    insta::assert_snapshot!(
        "complex_schemas_articles_resource_ts",
        extract_text(&tree, "src/resources/articles.ts")
    );

    run_tsx_test(&tree, include_str!("e2e_ts/complex_schemas_test.ts"));
}

#[test]
fn discriminated_union() {
    let ir = load_ir("discriminated_union.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("events-sdk"))
        .unwrap();

    insta::assert_snapshot!(
        "discriminated_union_models_ts",
        extract_text(&tree, "src/models.ts")
    );
    insta::assert_snapshot!(
        "discriminated_union_events_resource_ts",
        extract_text(&tree, "src/resources/events.ts")
    );

    run_tsx_test(&tree, include_str!("e2e_ts/discriminated_union_test.ts"));
}

#[test]
fn composition_api() {
    let ir = load_ir("composition_api.json");
    let tree = FetchGenerator
        .generate(&ir, &default_config("composition-sdk"))
        .unwrap();

    insta::assert_snapshot!(
        "composition_api_models_ts",
        extract_text(&tree, "src/models.ts")
    );
    insta::assert_snapshot!(
        "composition_api_composition_resource_ts",
        extract_text(&tree, "src/resources/composition.ts")
    );

    run_tsx_test(&tree, include_str!("e2e_ts/composition_api_test.ts"));
}

#[test]
fn circular_refs() {
    let ir = load_ir("circular_refs.yaml");
    let tree = FetchGenerator
        .generate(&ir, &default_config("tree-sdk"))
        .unwrap();

    insta::assert_snapshot!(
        "circular_refs_models_ts",
        extract_text(&tree, "src/models.ts")
    );
    insta::assert_snapshot!(
        "circular_refs_nodes_resource_ts",
        extract_text(&tree, "src/resources/nodes.ts")
    );

    run_tsx_test(&tree, include_str!("e2e_ts/circular_refs_test.ts"));
}
