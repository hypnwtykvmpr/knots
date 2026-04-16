# installed_workflows

Workflow and profile loading, parsing, and validation.

## Key Files

- **`mod.rs`** — public API, `WorkflowDefinition`, `PromptDefinition`, re-exports
- **`loader.rs`** — discover and load workflow bundles from `.knots/workflows/`
- **`operations.rs`** — `install_workflow()`, `uninstall_workflow()`, `write_repo_config()`
- **`registry.rs`** — `InstalledWorkflowRegistry`: lookup and resolution
- **`knot_type_registry.rs`** — `KnotTypeWorkflowConfig`, `WorkflowRef` mapping
- **`bundle_toml.rs`** / **`bundle_json.rs`** — TOML and JSON bundle parsing
- **`profile_toml.rs`** / **`profile_json.rs`** — profile definition parsing
- **`builtin.rs`** — built-in workflow bundle loading and prompt rendering
- **`loom.rs`** — integration with compiled loom bundles under `loom/*/dist/`
- **`ids.rs`** — `normalize_workflow_id()` and related helpers

## Key Types

- `WorkflowDefinition` — full workflow: states, transitions, profiles, prompts
- `ProfileDefinition` — states, transitions, owners, gates for a workflow variant
- `PromptDefinition` / `PromptParamDefinition` — action-state prompt templates
- `WorkflowRepoConfig` — on-disk representation of per-repo workflow config
