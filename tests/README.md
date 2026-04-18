# tests

Integration tests exercising the CLI and full application stack.

## Key Files

- **`cli_main_paths.rs`** — core create/update/state/list workflows
- **`cli_dispatch.rs`** — write operation dispatch and output formatting
- **`cli_dispatch_agent.rs`** / **`cli_dispatch_agent_lease.rs`** — poll, claim, next, lease flows
- **`cli_dispatch_gate.rs`** — gate evaluation and reopen flows
- **`cli_dispatch_metadata.rs`** — metadata visibility (notes, handoff capsules)
- **`cli_dispatch_sync.rs`** — push/pull/sync progress and JSON output
- **`cli_workflows.rs`** / **`cli_workflows_ext.rs`** — custom workflow install and runtime
- **`cli_state_hierarchy.rs`** / **`cli_auto_resolve_terminal_parents.rs`** — parent/child state cascading
- **`cli_skills.rs`** — managed `knots*` skill installation and doctor coverage
- **`cli_doctor_cold_tier_imbalance.rs`** / **`cli_doctor_terminal_parents.rs`** — doctor checks and `--fix` paths
- **`cli_rollback.rs`** — `kno rollback` and state rewind
- **`cli_named_projects.rs`** — project-scoped state and cross-project isolation
- **`cli_loom_profile_output.rs`** / **`cli_loom_prompt_resolution.rs`** — loom bundle integration
- **`cli_upgrade_notice.rs`** / **`cli_archival_sweep.rs`** / **`next_optimistic.rs`** — misc CLI behaviors
- **`repo_guardrails.rs`** — CLAUDE.md/AGENTS.md consistency, hook installation,
  pre-push guardrails

## Running

```sh
cargo test --all-targets --all-features
make sanity  # fmt + lint + test + coverage
make coverage  # coverage-only pass used by the managed pre-push hook
```

All tests use ephemeral temp directories. No external services required.
