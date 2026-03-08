# AGENTS.md

- Always write tests before writing code.
- Make sure the tests fail before the code is implemented.
- Make sure the tests pass after the code is written.

## Lint & Format

All rust code must be formatted and linted with

```bash
cargo fmt --all
cargo clippy --all-targets --all-features
```

## Build & Test

```bash
cargo build                          # build all crates
cargo test -p snapi-core             # IR, parser, resolver, normalizer
cargo test -p snapi-generators       # conformance + snapshot tests (requires tsc)
cargo test -p snapi-cli              # CLI integration tests
```

Conformance tests compile generated TypeScript via `tsc --noEmit`. Ensure `tsc` is available:

```bash
npm install
```

Do not attempt to edit `PATH`.

### Snapshot tests (insta)

When generated output changes intentionally, review and accept snapshots:

```bash
cargo insta review -p snapi-generators
```

Snapshots live in `crates/snapi-generators/tests/snapshots/`. Always commit reviewed snapshot changes alongside the code change that caused them.

---

## Architecture

The pipeline has three strictly separated stages:

```
OpenAPI spec → Parser → Resolver → Normalizer → IrApi → Generator → FileTree → disk
```

**Generators never touch `oas3` types.** They receive only `IrApi` (defined in `snapi-core/src/ir/`). Keep this boundary intact when adding generators or extending the IR.

**`IndexMap` is intentional everywhere** — it preserves insertion order for deterministic output. Do not swap for `HashMap`.

---

## Circular Reference Handling

The resolver's `visited` set is **stack-local**: entries are inserted on the way down and removed on the way back up. This prevents false positives on sibling fields. When a schema is encountered a second time *during active recursion*, the resolver emits `IrType::Recursive(name)` and stops. Do not change this to a persistent visited set.

---

## Adding a Generator

1. Implement `Generator` trait (`snapi-core/src/generator.rs`) in a new file under `crates/snapi-generators/src/`.
2. Register it in the generator registry in `snapi-cli/src/main.rs`.
3. Add at least one conformance test in `crates/snapi-generators/tests/conformance.rs` using an existing fixture or a new one in `tests/fixtures/`.

---

## Code Style

- Case conversions go through `snapi-core/src/utils/case.rs` (`to_snake_case`, `to_pascal_case`, `to_camel_case`, `escape_ident`). Do not call `heck` directly in generators.
- User-facing errors use `miette`. Add new variants to `SnapiError` in `snapi-core/src/error.rs` with `#[diagnostic]` hints.
- Tera templates for SDK boilerplate files (package.json, tsconfig.json, README) are embedded at compile time via `include_str!()`. Runtime template strings for code generation are fine inline.
- Avoid using unchecked casts in all code and tests.
- Functions should be short and have low branch complexity.

---

## Test Fixtures

OpenAPI fixtures are in `crates/snapi-generators/tests/fixtures/`. When adding a new fixture, prefer YAML.
