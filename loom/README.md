# loom

Source workflow definitions written in the `loom` DSL. Each subdirectory is a bundle that compiles to a JSON artifact embedded or loaded by the `kno` binary.

## Bundles

- **`work_sdlc/`** — standard software delivery lifecycle (plan → implement → ship)
- **`execution_plan_sdlc/`** — plan-authoring workflow with wave/step edits
- **`explore_sdlc/`** — exploration/spike workflow
- **`gate_sdlc/`** — gate-review workflow for Gate-typed knots
- **`lease_sdlc/`** — agent lease acquisition and handoff

## Bundle Layout

Every bundle has the same structure:

- **`workflow.loom`** — top-level workflow definition (states, transitions, gates)
- **`loom.toml`** — bundle metadata: name, version, entry, default profile
- **`profiles/*.loom`** — profile variants (autopilot, semiauto, etc.) that specialize ownership and automation
- **`prompts/*.md`** — per-action-state prompt templates rendered for agents
- **`dist/bundle.json`** — compiled artifact produced by `make loom-bundle`

## Building

```sh
make loom-bundle   # compiles loom/work_sdlc to dist/bundle.json
```

`src/installed_workflows/loom.rs` loads compiled bundles at runtime. All workflow step prompts must live here — the managed skills in `skills/` only describe the `kno` CLI.
