# Knots Architecture v5

This is the current architecture deep dive for Knots. It replaces
`knots-architecture-v4.md` as the canonical design document.

Read [TAXONOMY.md](TAXONOMY.md) first for the shared vocabulary used here.
That file is the canonical naming guide for knot, gate, lease, wave, step,
queue state, action state, terminal state, and related terms.

For state identifiers specifically, the implementation is driven by compiled
workflow bundles under [`loom/`](loom/README.md) and surfaced through
[`ProfileDefinition`](src/profile.rs) plus the helpers in
[`src/domain/state.rs`](src/domain/state.rs). There is no hand-maintained Rust
enum that defines the full state set.

## 1. System Shape

Knots is a local-first workflow tracker with four core traits:

- Events are the source of truth.
- SQLite is a rebuildable materialized cache.
- Git is the replication transport.
- Workflow bundles define states, prompts, ownership, and outputs.

At a high level, the runtime looks like this:

```text
CLI / read commands
  -> open App with repo context
  -> answer from SQLite projections

CLI / write commands
  -> write_dispatch
  -> write_queue (FIFO serialization)
  -> App methods
  -> append events + update SQLite

Sync / replication
  -> sync module pulls and applies remote events
  -> replication module stages local events into the git worktree
  -> git pushes and fetches the Knots branch
```

The main subsystem responsibilities are:

- CLI: parse commands and open repo context.
  Key paths: `src/main.rs`, `src/cli*.rs`
- App: perform domain operations on knots.
  Key paths: [`src/app/`](src/app/README.md)
- Workflows: load loom bundles and profiles.
  Key paths:
  [`src/installed_workflows/`](src/installed_workflows/README.md),
  [`loom/`](loom/README.md)
- Events: append immutable JSON event files.
  Key paths: [`src/events/`](src/events/README.md)
- Cache: serve fast local reads from SQLite.
  Key paths: [`src/db/`](src/db/README.md)
- Sync: pull, apply, and push event data through git.
  Key paths: [`src/sync/`](src/sync/README.md), `src/replication.rs`
- Write serialization: prevent concurrent local writes.
  Key paths:
  `src/write_queue.rs`, [`src/write_dispatch/`](src/write_dispatch/README.md)

## 2. Workflow Model

### 2.1 Bundles, Profiles, and State Kinds

The architecture is workflow-first. Each bundled workflow ships a compiled
`dist/bundle.json` that declares:

- the state ids and their kinds
- legal transitions
- per-action prompts
- executor ownership
- output artifact expectations

`ProfileDefinition` overlays that workflow data with ownership, output mode,
review hints, and alias handling. That is why the same workflow can support
profiles such as `autopilot`, `autopilot_with_pr`, `semiauto`, and
`autopilot_no_planning` without changing the underlying state graph.

Knots currently relies on four state kinds:

- `queue`: waiting to be picked up
- `action`: currently in active work
- `escape`: paused outside the main happy path
- `terminal`: finished for lifecycle purposes

### 2.2 Current Bundled State Sets

The current built-in bundles define these state ids.

#### `work_sdlc`

- Queue:
  `ready_for_planning`, `ready_for_plan_review`,
  `ready_for_implementation`, `ready_for_implementation_review`,
  `ready_for_shipment`, `ready_for_shipment_review`
- Action:
  `planning`, `plan_review`, `implementation`, `implementation_review`,
  `shipment`, `shipment_review`
- Escape: `blocked`, `deferred`
- Terminal: `shipped`, `abandoned`

#### `execution_plan_sdlc`

- Queue:
  `ready_for_design`, `ready_for_review`, `ready_for_orchestration`
- Action: `design`, `review`, `orchestration`
- Escape: `blocked`, `deferred`
- Terminal: `shipped`, `abandoned`

#### `explore_sdlc`

- Queue: `ready_for_exploration`
- Action: `exploration`
- Escape: `deferred`
- Terminal: `shipped`, `abandoned`

#### `gate_sdlc`

- Queue: `ready_to_evaluate`
- Action: `evaluating`
- Escape: `deferred`
- Terminal: `shipped`, `abandoned`

#### `lease_sdlc`

- Queue: `lease_ready`
- Action: `lease_active`
- Terminal: `lease_terminated`

These ids come from the current compiled bundles, not from prose in this file.
Use [TAXONOMY.md](TAXONOMY.md) for shared meaning and the bundle JSON for exact
built-in definitions.

### 2.3 Output Modes and Ownership

Profiles also decide who owns each action and what artifact a step is supposed
to produce. In the bundled workflows today, common output modes are:

- `remote_main`: feature branch pushed for review, then later merged to `main`
- `pr`: pull request is the review artifact
- `branch`: pushed branch is the final artifact
- `live_deployment`: build is prepared for deployment review

Prompt rendering happens from loom-authored markdown templates. A claim is not a
generic task wrapper; it is a workflow-defined prompt for one action state.

### 2.4 Legacy Aliases

The current implementation still resolves several historical labels through
`ProfileDefinition::state_aliases` and `resolve_state()`:

| Historical label | Current handling |
|------------------|------------------|
| `idea` | alias for `ready_for_planning` |
| `work_item` | alias for `ready_for_implementation` |
| `implementing` | alias for `implementation` |
| `implemented` | alias for `ready_for_implementation_review` |
| `reviewing` | alias for `implementation_review` |
| `approved` | alias for `ready_for_shipment` |
| `shipping` | alias for `shipment` |
| `rejected` | not a canonical state; use review-decision metadata plus routing |
| `refining` | not a canonical state; current flows return to queue states instead |

Those labels are compatibility affordances only. New docs and new code should
use the canonical state ids from the active workflow bundle.

### 2.5 The `blocked` Divergence

[TAXONOMY.md](TAXONOMY.md) describes `blocked` as an edge concept, while the
current `work_sdlc` and `execution_plan_sdlc` bundles still emit `blocked` as
an `escape` state. v5 documents that divergence instead of hiding it:

- the taxonomy is the preferred naming guide for humans
- the bundles are the executable definition used by the runtime today

That mismatch should be resolved in workflow source, not papered over in docs.

## 3. Event Model

Knots persists every write as append-only JSON under `.knots/`. The runtime uses
two streams:

- Full stream: `.knots/events/YYYY/MM/DD/<uuid>-<type>.json`
- Index stream: `.knots/index/YYYY/MM/DD/<uuid>-idx.knot_head.json`

The full stream holds the durable record. The index stream carries lightweight
head snapshots so listing and sync do not need to fetch every full payload.

### 3.1 Full Event Kinds

`FullEventKind` currently defines 22 event kinds:

- Creation and content:
  `knot.created`, `knot.title_set`, `knot.body_set`,
  `knot.description_set`, `knot.acceptance_set`
- Workflow and routing:
  `knot.state_set`, `knot.priority_set`, `knot.type_set`,
  `knot.profile_set`, `knot.review_decision`
- Notes and tags:
  `knot.comment_added`, `knot.note_added`,
  `knot.handoff_capsule_added`, `knot.tag_add`, `knot.tag_remove`
- Specialization and relations:
  `knot.invariants_set`, `knot.gate_data_set`,
  `knot.execution_plan_data_set`, `knot.edge_add`, `knot.edge_remove`,
  `knot.lease_data_set`, `knot.lease_id_set`

The index stream currently has one kind: `idx.knot_head`.

### 3.2 Event Identity and Ordering

Event ids are UUIDv7 values so file names sort by time while remaining globally
unique. The event writer computes a dated path from `occurred_at` and the event
id, then writes once. Existing event files are treated as immutable history.

### 3.3 Review Decisions and Handoff Capsules

Review outcomes are not represented only by state names. They also appear in the
event log through `knot.review_decision`, and step-to-step context moves through
`knot.handoff_capsule_added`. That split is important: routing happens through
workflow transitions, while explanation and reviewer guidance live in metadata.

### 3.4 Optimistic Concurrency

Workflow-sensitive writes can carry `WorkflowPrecondition { profile_etag }`.
This is the architecture's optimistic concurrency guard:

- the caller reads the current workflow etag
- the write attaches that etag as a precondition
- the write fails if the knot moved under another actor first

That keeps concurrent agents from silently overwriting one another's workflow
position.

## 4. Local Cache and Storage Tiers

SQLite is the read-optimized projection layer. The cache is not authoritative;
it can be rebuilt from events through rehydration.

Important properties of the cache layer today:

- WAL mode is enabled
- busy timeout is 5 seconds
- foreign keys are enabled
- schema versioning is managed through sequential migrations

Knots uses tiered storage for read performance:

- Hot tier: full knot rows for recently active knots
- Warm tier: lighter summaries and catalog-style lookups
- Cold tier: terminal-state catalog entries that can be rehydrated on demand

`src/app/archival.rs` runs the cold sweep that moves terminal knots out of the
hot path, and `src/app/rehydrate.rs` rebuilds projections when data must be
read back from event history.

## 5. Sync, Replication, and Worktrees

Git is the transport layer for shared Knots data. The current implementation
uses a dedicated worktree for the internal Knots branch and keeps event payloads
out of normal feature-branch diffs.

The sync flow is intentionally asymmetric:

- `pull`: fetch remote updates, reset the Knots worktree, apply index events,
  then apply full events into SQLite
- `push`: scan local events, copy them into the Knots worktree, commit, push

This gives Knots local-first writes without requiring direct remote access on
every command. The local repo remains the working set; git is the replication
mechanism, not the primary query engine.

## 6. Locks and the Write Queue

Knots protects local consistency with explicit serialization rules.

- All write commands route through `write_dispatch`
- Writes are serialized by the FIFO `write_queue`
- Lock ordering is `repo.lock` first, then `cache.lock`
- Readers can keep using SQLite without taking the full write path

The lock ordering rule matters because both git and SQLite participate in a
single logical write. Reversing that order risks deadlocks and partially applied
state.

## 7. Knot Types Beyond Standard Work

The architecture is no longer just a work tracker.

### Gate knots

Gate knots carry `GateData`, invariants, owner kind, and failure routing. Their
workflow is compact: `ready_to_evaluate` -> `evaluating` -> terminal or routed
reopen states on related work.

### Lease knots

Leases are first-class knots that represent claim ownership. A claim activates a
lease, binds it to the active knot, and blocks sync while the lease remains
live. `kno next`, explicit termination, or timeout moves the lease to
`lease_terminated`.

See [docs/leases.md](docs/leases.md) for the full lifecycle.

### Exploration knots

Exploration knots support bounded investigation work without forcing that work
into the full plan / build / ship lifecycle.

### Execution-plan knots

Execution-plan knots are orchestration artifacts, not units of feature work.
Their domain model is:

- waves run sequentially
- steps inside a wave run sequentially
- knot ids inside one step form the concurrent work set

See [docs/execution-plans.md](docs/execution-plans.md) for the user-facing plan
editing model.

## 8. Integrity and Recovery

`kno doctor` is the primary runtime integrity check. The current built-in checks
include:

- `lock_health`
- `worktree`
- `remote`
- `version`
- `hooks`
- `registered_workflows`
- `schema_version`
- `stuck_leases`
- `terminal_parents`
- `cold_tier_imbalance`

Managed-skill checks can add more entries to the report.

The recovery posture is straightforward:

- event files are immutable and authoritative
- SQLite can be rebuilt
- expired leases can be materialized and cleaned up
- terminal-parent drift is detectable
- workflow registration problems surface through doctor before execution

## 9. Historical Note and Open Items

`knots-architecture-v4.md` remains in the repo as historical context, but its
state terminology is no longer canonical.

Two open items shape the next architecture revision:

1. Bundle-driven state ids may later be code-generated into Rust types. Until
   that lands, the executable state source remains loom bundle output plus
   `ProfileDefinition`.
2. The `blocked` edge-versus-state discrepancy should be resolved in workflow
   source and taxonomy together so docs, bundles, and runtime helpers agree.
