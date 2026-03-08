# snapi Design Document

**Date:** 2026-03-08
**Status:** Approved

## Overview

`snapi` is a CLI tool that generates idiomatic, publishable SDKs from OpenAPI 3.1 specifications. It targets TypeScript, Rust, Go, and Python, with a clean extension model for adding new languages. The guiding principle is that generated SDKs must feel as good as hand-written ones — which requires programmatic generation, not just templates.

---

## 1. Architecture

```
snapi.toml + openapi.yaml
         │
         ▼
   ┌─────────────┐
   │   Parser    │  Reads & validates OpenAPI 3.1 spec (via `oas3` crate)
   └──────┬──────┘
          │
          ▼
   ┌─────────────┐
   │  Resolver   │  Recursively follows all $ref (local + external files)
   └──────┬──────┘
          │
          ▼
   ┌─────────────┐
   │ Normalizer  │  Flattens allOf, resolves discriminators, detects circular
   └──────┬──────┘  refs, normalizes type:["X","null"] → Optional(X)
          │  clean IrApi
          ▼
   ┌─────────────────────────────────────┐
   │           Generator trait           │
   │  ┌──────────┐  ┌──────┐  ┌──────┐  │
   │  │TypeScript│  │ Rust │  │  Go  │  │  one impl per language/variant
   │  └──────────┘  └──────┘  └──────┘  │
   └──────────────────┬──────────────────┘
                      │ FileTree
                      ▼
              ┌───────────────┐
              │  File writer  │  Renders Tera templates for boilerplate,
              └───────────────┘  writes output tree to disk
```

### Crate structure

```
snapi/
├── crates/
│   ├── snapi-core/         # Parser, IR types, Generator trait, resolver,
│   │                       # normalizer, file writer, shared utilities
│   ├── snapi-cli/          # clap CLI, reads snapi.toml, orchestrates generation
│   └── snapi-generators/   # One module per language, one file per variant
│       ├── src/
│       │   ├── typescript/
│       │   │   ├── mod.rs      # shared type/model rendering
│       │   │   ├── fetch.rs    # FetchGenerator (default)
│       │   │   └── axios.rs    # AxiosGenerator
│       │   ├── rust/
│       │   │   ├── mod.rs
│       │   │   ├── reqwest.rs  # ReqwestGenerator (default)
│       │   │   └── ureq.rs     # UreqGenerator
│       │   └── python/
│       │       ├── mod.rs
│       │       ├── httpx.rs    # HttpxGenerator (default)
│       │       └── requests.rs # RequestsGenerator
│       └── templates/      # Tera templates for boilerplate files only
└── Cargo.toml              # workspace
```

### Key crates

| Crate | Purpose |
|---|---|
| `oas3` (x52dev/oas3-rs) | OpenAPI 3.1 parsing |
| `serde` / `serde_yaml` / `serde_json` | Deserialization |
| `tera` | Templates for boilerplate (Cargo.toml, package.json, README) |
| `clap` | CLI argument parsing |
| `miette` | User-facing error reporting with source annotations |
| `anyhow` | Internal error propagation |
| `heck` | Case conversion across all generators |
| `indexmap` | Ordered maps for deterministic output |
| `insta` | Snapshot testing for generator output |

---

## 2. Intermediate Representation (IR)

The IR is a fully resolved, language-agnostic semantic model. Generators never inspect `oas3` types directly — they work exclusively against the IR. This insulates generators from OpenAPI parsing details and version quirks.

### Schema types

```rust
pub enum IrType {
    // Primitives
    String(IrStringConstraints),    // min/maxLength, pattern, format (date, uuid, etc.)
    Integer(IrIntConstraints),      // min/max, format (int32, int64)
    Float(IrFloatConstraints),
    Boolean,
    Null,

    // Composites
    Array { items: Box<IrType>, min: Option<u64>, max: Option<u64> },
    Map(Box<IrType>),               // additionalProperties: { type: X }
    Object(IrObject),

    // Algebraic
    Enum(IrEnum),                   // oneOf + discriminator → tagged union
    Union(Vec<IrType>),             // anyOf/oneOf without discriminator
    Intersection(Vec<IrObject>),    // allOf → merged struct

    // Special
    Optional(Box<IrType>),          // type: ["X", "null"]
    Recursive(String),              // circular ref — back-reference by schema name
    Any,                            // additionalProperties: true / schema: {}
}
```

### Circular reference handling

The resolver tracks a visited set (`HashSet<SchemaRef>`) as it recurses. On encountering a `$ref` already in the set, it emits `IrType::Recursive(name)` and stops. Entries are removed on the way back up so sibling fields are not falsely flagged.

`Recursive(name)` holds the name of the schema in `IrApi::schemas` where the full definition lives. Generators emit language-appropriate indirection:

| Language | Strategy |
|---|---|
| Rust | `Box<T>` |
| TypeScript | Plain reference (already indirect) |
| Python | Forward reference string or `from __future__ import annotations` |
| Go | `*T` pointer |

### Operations

```rust
pub struct IrOperation {
    pub id: String,              // operationId, normalized to snake_case
    pub method: HttpMethod,
    pub path: String,
    pub summary: Option<String>,
    pub deprecated: bool,
    pub params: Vec<IrParam>,
    pub request_body: Option<IrRequestBody>,
    pub responses: Vec<IrResponse>,
    pub auth: Vec<IrAuthRequirement>,
    pub tags: Vec<String>,       // used to group operations into resource modules
}

pub struct IrParam {
    pub name: String,
    pub location: ParamLocation, // Path, Query, Header, Cookie
    pub ty: IrType,
    pub required: bool,
}
```

### Top-level model

```rust
pub struct IrApi {
    pub title: String,
    pub version: String,
    pub base_url: Option<String>,
    pub schemas: IndexMap<String, IrType>,   // all named schemas, fully resolved
    pub operations: Vec<IrOperation>,
    pub auth_schemes: Vec<IrAuthScheme>,
    pub webhooks: Vec<IrWebhook>,            // OpenAPI 3.1 native webhooks
}
```

---

## 3. Generator Trait & Extensibility

### Trait

```rust
pub trait Generator {
    fn language(&self) -> &'static str;      // e.g. "typescript"
    fn variant(&self) -> &'static str;       // e.g. "fetch", "axios"; "default" for canonical
    fn description(&self) -> &'static str;   // shown in `snapi list-generators`
    fn generate(&self, api: &IrApi, config: &TargetConfig) -> Result<FileTree>;
}

pub struct FileTree {
    pub files: IndexMap<PathBuf, FileContent>,
}

pub enum FileContent {
    Text(String),
    Template { name: &'static str, context: tera::Context },
}
```

Generators produce a virtual `FileTree` and never write to disk directly. The file writer flushes the tree, keeping generation logic pure and easily testable.

### Multiple variants per language

Each language module exposes shared rendering functions (`render_types`, `render_models`, `render_resources`) used by all variants of that language. Only `render_client` differs between variants. This avoids duplication while keeping variants independent.

### Operation grouping

Operations are grouped into resource modules by tag (`IrOperation::tags`). A `users` tag produces a `UsersResource` class/struct/module. Untagged operations go into a `default` resource.

### Adding a new language

1. Create `crates/snapi-generators/src/<lang>/mod.rs` (shared rendering)
2. Create `crates/snapi-generators/src/<lang>/<variant>.rs` (implements `Generator`)
3. Add Tera templates to `crates/snapi-generators/templates/<lang>/`
4. Register in the generator registry (one line)

### Registry

```rust
pub fn registry() -> Vec<Box<dyn Generator>> {
    vec![
        Box::new(typescript::FetchGenerator),
        Box::new(typescript::AxiosGenerator),
        Box::new(rust::ReqwestGenerator),
        Box::new(rust::UreqGenerator),
        Box::new(python::HttpxGenerator),
        Box::new(python::RequestsGenerator),
    ]
}
```

### Shared utilities (in `snapi-core`)

```rust
pub fn to_snake_case(s: &str) -> String;
pub fn to_pascal_case(s: &str) -> String;
pub fn to_camel_case(s: &str) -> String;
pub fn escape_ident(s: &str, lang: Language) -> String;
pub fn format_doc(description: &str, style: DocStyle) -> String;
// DocStyle: TripleSlash (Rust), JsDoc (TypeScript), GoogleStyle (Python/Go)
```

---

## 4. Configuration Schema

### `snapi.toml`

```toml
[input]
spec = "openapi.yaml"          # path to OpenAPI 3.1 spec (YAML or JSON)

[package]
version = "0.1.0"              # shared across all targets, overridable per target

[[target]]
language    = "typescript"
variant     = "fetch"          # optional — omit to use language's default variant
dir         = "./sdks/typescript"
name        = "my-api-client"
description = "TypeScript client for My API"  # fallback: spec info.description
version     = "0.2.0"         # optional — overrides [package].version

[target.publish]
registry = "npm"               # npm | crates | pypi | none
access   = "public"            # npm-specific

[[target]]
language = "rust"
variant  = "reqwest"
dir      = "./sdks/rust"
name     = "my-api-client"

[target.publish]
registry = "crates"
```

### Design decisions

- `[package].version` is a shared default; targets override when needed (e.g. a per-language hotfix)
- `[target.publish]` is metadata only — `snapi` populates `Cargo.toml`/`package.json`/`pyproject.toml` but does not run publish commands. Publishing is left to the user's CI pipeline.
- `info.title`, `info.description`, `info.version` from the OpenAPI spec are used as fallbacks, enabling minimal configs.

### Minimal valid config

```toml
[input]
spec = "openapi.yaml"

[[target]]
language = "typescript"
dir      = "./sdks/typescript"
```

### CLI commands

```
snapi generate                        # generate all [[target]] blocks
snapi generate --target typescript    # generate one language only
snapi list-generators                 # print all available language/variant pairs
snapi validate                        # parse + validate spec and config, no output
snapi dump-ir                         # debug: print normalized IR as JSON
```

---

## 5. SDK Features

### Priority

| Feature | Priority |
|---|---|
| Type-safe request/response models | Must-have (v1) |
| Authentication (API key, Bearer, OAuth2) | Must-have (v1) |
| Streaming (SSE, chunked responses) | Must-have (slightly deferred) |
| Webhooks (type-safe payload parsing) | Must-have (slightly deferred) |
| Retries with backoff | Nice-to-have |
| Pagination (cursor, offset, page) | Nice-to-have |

### Authentication

Auth schemes from `IrAuthScheme` are wired into the generated client constructor. Users pass credentials at construction time; the SDK attaches them to every request automatically.

---

## 6. Error Handling

Two distinct error categories:

**User-facing errors** use `miette` to render annotated source snippets:

```
Error: Missing required field 'operationId'
  --> openapi.yaml:42:5
   |
42 |     get:
   |     ^^^ every operation must have a unique operationId for SDK generation
   |
   = hint: add `operationId: listUsers` to this operation
```

**Internal errors** use `anyhow` for ergonomic propagation. These indicate bugs, not user mistakes.

### Error taxonomy

```rust
pub enum SnapiError {
    ConfigNotFound(PathBuf),
    ConfigParse(miette::Report),
    SpecNotFound(PathBuf),
    SpecParse(miette::Report),
    UnresolvableRef { ref_path: String, location: SourceSpan },
    CircularRefNotNamed { location: SourceSpan },
    UnsupportedConstruct { what: String, location: SourceSpan },
    UnknownGenerator { language: String, variant: String },
    GeneratorFailed { language: String, source: anyhow::Error },
    OutputWriteFailed { path: PathBuf, source: std::io::Error },
}
```

### `UnsupportedConstruct`

A small number of valid OpenAPI constructs have no unambiguous idiomatic mapping (e.g. `anyOf` without a `discriminator` across many types). Rather than silently generating broken or type-unsafe code, `snapi` emits `UnsupportedConstruct` with the exact spec location and a link to a tracking issue.

This is a temporary, tracked gap — not a permanent escape hatch. The conformance test corpus drives these to zero over time. Silently generating wrong code is strictly worse than a clear error.

---

## 7. Testing Strategy

### Unit tests

- IR normalizer: hand-crafted `oas3` structs → assert expected `IrApi` shape
- Resolver: circular ref detection, external `$ref` loading, edge cases
- Each generator: hand-crafted `IrApi` fixtures → assert exact file contents

### Snapshot tests (`insta` crate)

A set of curated `IrApi` fixtures covering:
- Simple CRUD operations
- Pagination
- Discriminated unions (`oneOf` + `discriminator`)
- Recursive/circular schemas
- Streaming responses
- Webhooks

Generator output is snapshotted and committed. CI fails on any unreviewed diff, catching regressions without brittle string assertions.

### Conformance tests

End-to-end tests against a corpus of real-world OpenAPI 3.1 specs (Stripe, GitHub, Petstore extended, OpenAI). For each spec and each target language:

1. `snapi generate` runs against the spec
2. Output is compiled with the target toolchain (`tsc --noEmit`, `cargo check`, `mypy`, `go build`)
3. Compilation failure = bug, tracked as a GitHub issue

Conformance tests run in CI and are the ultimate correctness gate for the full-spec-compliance requirement.
