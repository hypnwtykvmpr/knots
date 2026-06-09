use std::{io, io::BufRead, io::IsTerminal, io::Write};

use crate::app::{App, AppError, GateDecision};
use crate::dispatch::knot_ref;
use crate::domain::execution_plan_edit::CascadeInfo;
use crate::domain::gate::{parse_failure_mode_spec, GateData, GateOwnerKind};
use crate::domain::knot_type::KnotType;
use crate::domain::state::normalize_state_input;
use crate::ui;

const CLAIM_ONLY_LEASE_BINDING: &str = "lease binding is only allowed during claim operations";

pub(crate) fn resolve_lease_agent_info(
    app: &App,
    knot_id: &str,
) -> Option<crate::domain::lease::AgentInfo> {
    let knot = app.show_knot(knot_id).ok()??;
    let lease_id = knot.lease_id.as_ref()?;
    let lease_knot = app.show_knot(lease_id).ok()??;
    lease_knot.lease.as_ref()?.agent_info.clone()
}

/// Emit the standard three-line deprecation warning for agent metadata flags
/// that are now declared by the lease. Callers pass the CLI subcommand name
/// (e.g. "next", "update", "gate evaluate"), the list of flag names the user
/// actually supplied (empty = no warning), and whether a lease is currently
/// bound to the target knot.
///
/// Per the lease-as-declared-identity contract, these flags are still parsed
/// by clap but their values are ignored at runtime; identity comes from the
/// bound lease's agent_info.
pub(crate) fn warn_deprecated_agent_metadata(
    command: &str,
    supplied_flags: &[&str],
    lease_bound: bool,
) {
    if supplied_flags.is_empty() {
        return;
    }
    let joined = supplied_flags
        .iter()
        .map(|f| format!("--{f}"))
        .collect::<Vec<_>>()
        .join(", ");
    let (verb, pronoun) = if supplied_flags.len() == 1 {
        ("is", "this value is")
    } else {
        ("are", "these values are")
    };
    eprintln!(
        "warning: {joined} {verb} deprecated on `kno {command}` and will be rejected as an error \
         in a future release"
    );
    eprintln!("{pronoun} ignored; agent identity is taken from the active lease on the knot");
    if !lease_bound {
        eprintln!(
            "no lease is bound to this knot \u{2014} create one with `kno lease create` and \
             pass `--lease <id>` on `kno claim`"
        );
    }
}

/// Build a `StateActorMetadata` that carries lease-sourced identity. When the
/// knot has no bound lease, agent fields are `None`; callers must rely on the
/// deprecation warning to tell the user why.
pub(crate) fn lease_state_actor(
    app: &App,
    knot_id: &str,
    actor_kind: Option<String>,
) -> crate::app::StateActorMetadata {
    state_actor_from_agent_info(actor_kind, resolve_lease_agent_info(app, knot_id).as_ref())
}

/// Translate a lease's `agent_info` into a `StateActorMetadata`. When no
/// `agent_info` is available (no lease, or a lease without agent identity)
/// the returned metadata has unset agent fields.
pub(crate) fn state_actor_from_agent_info(
    actor_kind: Option<String>,
    info: Option<&crate::domain::lease::AgentInfo>,
) -> crate::app::StateActorMetadata {
    crate::app::StateActorMetadata {
        actor_kind,
        agent_name: info.map(|i| i.agent_name.clone()).filter(|s| !s.is_empty()),
        agent_model: info.map(|i| i.model.clone()).filter(|s| !s.is_empty()),
        agent_version: info
            .map(|i| i.model_version.clone())
            .filter(|s| !s.is_empty()),
    }
}

/// Return the list of deprecated agent metadata flag names the caller
/// actually supplied, using the standard `--agent-*` spelling.
pub(crate) fn supplied_agent_flag_names<'a>(
    agent_name: Option<&str>,
    agent_model: Option<&str>,
    agent_version: Option<&str>,
) -> Vec<&'a str> {
    let mut flags = Vec::new();
    if agent_name.is_some() {
        flags.push("agent-name");
    }
    if agent_model.is_some() {
        flags.push("agent-model");
    }
    if agent_version.is_some() {
        flags.push("agent-version");
    }
    flags
}

pub(crate) fn validate_non_claim_lease(
    knot: &crate::app::KnotView,
    lease_id: Option<&str>,
) -> Result<(), AppError> {
    let Some(provided_lease) = lease_id else {
        return Ok(());
    };

    match knot.lease_id.as_deref() {
        Some(knot_lease) if knot_lease == provided_lease => Ok(()),
        Some(knot_lease) => Err(AppError::InvalidArgument(format!(
            "lease mismatch: knot has '{knot_lease}', caller provided '{provided_lease}'"
        ))),
        None => Err(AppError::InvalidArgument(format!(
            "knot has no active lease but caller provided \
             '{provided_lease}'; {CLAIM_ONLY_LEASE_BINDING}"
        ))),
    }
}

pub(crate) fn execute_with_terminal_cascade_prompt<T, F>(
    preapproved: bool,
    mut action: F,
) -> Result<T, AppError>
where
    F: FnMut(bool) -> Result<T, AppError>,
{
    let mut approved = preapproved;
    loop {
        match action(approved) {
            Ok(value) => return Ok(value),
            Err(AppError::TerminalCascadeApprovalRequired {
                knot_id,
                target_state,
                descendants,
            }) if !approved => {
                if !io::stdin().is_terminal() {
                    return Err(AppError::TerminalCascadeApprovalRequired {
                        knot_id,
                        target_state,
                        descendants,
                    });
                }
                if prompt_for_terminal_cascade_approval(&knot_id, &target_state, &descendants)? {
                    approved = true;
                    continue;
                }
                return Err(AppError::InvalidArgument(
                    "terminal cascade cancelled; no changes written".to_string(),
                ));
            }
            Err(err) => return Err(err),
        }
    }
}

fn prompt_for_terminal_cascade_approval(
    knot_id: &str,
    target_state: &str,
    descendants: &[crate::state_hierarchy::HierarchyKnot],
) -> Result<bool, AppError> {
    let mut stderr = io::stderr();
    let mut stdin = io::stdin().lock();
    terminal_cascade_prompt(&mut stderr, &mut stdin, knot_id, target_state, descendants)
}

pub(super) fn terminal_cascade_prompt<W: Write, R: BufRead>(
    writer: &mut W,
    reader: &mut R,
    knot_id: &str,
    target_state: &str,
    descendants: &[crate::state_hierarchy::HierarchyKnot],
) -> Result<bool, AppError> {
    writeln!(
        writer,
        "moving '{}' to '{}' will also move descendant knots \
         to that terminal state:",
        knot_id, target_state
    )?;
    writeln!(
        writer,
        "  {}",
        crate::state_hierarchy::format_hierarchy_knots(descendants)
    )?;
    write!(writer, "continue? [y/N]: ")?;
    writer.flush()?;

    let mut input = String::new();
    reader.read_line(&mut input)?;
    Ok(is_terminal_cascade_approval(&input))
}

pub(crate) fn plan_cascade_prompt<W: Write, R: BufRead>(
    writer: &mut W,
    reader: &mut R,
    summary: &str,
    cascade: &CascadeInfo,
) -> Result<bool, AppError> {
    writeln!(
        writer,
        "{summary} will cascade delete {} step(s) and affect {} knot id(s):",
        cascade.step_count,
        cascade.affected_knot_ids.len()
    )?;
    if !cascade.affected_knot_ids.is_empty() {
        writeln!(writer, "  {}", cascade.affected_knot_ids.join(", "))?;
    }
    write!(writer, "continue? [y/N]: ")?;
    writer.flush()?;

    let mut input = String::new();
    reader.read_line(&mut input)?;
    Ok(is_terminal_cascade_approval(&input))
}

pub(super) fn is_terminal_cascade_approval(input: &str) -> bool {
    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

pub(crate) fn format_json(value: &serde_json::Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(value).expect("queued json serialization should succeed")
    )
}

pub(crate) fn parse_gate_decision(raw: &str) -> Result<GateDecision, AppError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "yes" | "pass" => Ok(GateDecision::Yes),
        "no" | "fail" => Ok(GateDecision::No),
        _ => Err(AppError::InvalidArgument(
            "--decision must be one of: yes, no".to_string(),
        )),
    }
}

pub(crate) fn parse_knot_type_arg(raw: Option<&str>) -> Result<KnotType, AppError> {
    raw.unwrap_or("work")
        .parse::<KnotType>()
        .map_err(|err| AppError::InvalidArgument(err.to_string()))
}

pub(crate) fn parse_gate_owner_kind_arg(
    raw: Option<&str>,
) -> Result<Option<GateOwnerKind>, AppError> {
    raw.map(|value| {
        value
            .parse::<GateOwnerKind>()
            .map_err(|err| AppError::InvalidArgument(err.to_string()))
    })
    .transpose()
}

pub(crate) fn parse_gate_failure_modes_option(
    raw_specs: &[String],
) -> Result<Option<std::collections::BTreeMap<String, Vec<String>>>, AppError> {
    if raw_specs.is_empty() {
        return Ok(None);
    }
    let mut failure_modes = std::collections::BTreeMap::new();
    for raw in raw_specs {
        let (invariant, targets) = parse_failure_mode_spec(raw)
            .map_err(|err| AppError::InvalidArgument(err.to_string()))?;
        failure_modes.insert(invariant, targets);
    }
    Ok(Some(failure_modes))
}

pub(crate) fn parse_gate_data_args(
    owner_kind: Option<&str>,
    raw_failure_modes: &[String],
    knot_type: KnotType,
) -> Result<GateData, AppError> {
    let owner_kind = parse_gate_owner_kind_arg(owner_kind)?;
    let failure_modes = parse_gate_failure_modes_option(raw_failure_modes)?.unwrap_or_default();
    if knot_type != KnotType::Gate && (owner_kind.is_some() || !failure_modes.is_empty()) {
        return Err(AppError::InvalidArgument(
            "gate owner/failure mode fields require knot type 'gate'".to_string(),
        ));
    }
    Ok(GateData {
        owner_kind: owner_kind.unwrap_or_default(),
        failure_modes,
    })
}

pub(crate) fn normalize_expected_state(raw: &str) -> String {
    // Normalize formatting (trim, lowercase, dash→underscore). Legacy alias
    // resolution (e.g. `implemented` → `ready_for_implementation_review`)
    // happens at the profile boundary via `domain::state::resolve_state`.
    // Callers here compare against a raw knot state string and do not have a
    // profile in scope, so we only canonicalize the formatting.
    let normalized = normalize_state_input(raw);
    match normalized.as_str() {
        "idea" => "ready_for_planning".to_string(),
        "work_item" | "rejected" | "refining" => "ready_for_implementation".to_string(),
        "implementing" => "implementation".to_string(),
        "implemented" => "ready_for_implementation_review".to_string(),
        "reviewing" => "implementation_review".to_string(),
        "evaluate" => "evaluating".to_string(),
        "exploring" => "exploration".to_string(),
        "approved" => "ready_for_shipment".to_string(),
        "shipping" => "shipment".to_string(),
        _ => normalized,
    }
}

pub(crate) fn format_next_output(
    knot: &crate::app::KnotView,
    previous_state: &str,
    owner_kind: Option<&str>,
    json: bool,
) -> String {
    if json {
        let result = serde_json::json!({
            "id": &knot.id,
            "previous_state": previous_state,
            "state": &knot.state,
            "owner_kind": owner_kind,
        });
        return format_json(&result);
    }
    let palette = ui::Palette::auto();
    let owner_suffix = owner_kind
        .map(|kind| format!(" (owner: {kind})"))
        .unwrap_or_default();
    format!(
        "updated {} -> {}{}\n",
        palette.id(&knot_ref(knot)),
        palette.state(&knot.state),
        owner_suffix,
    )
}

pub(crate) fn format_rollback_output(
    knot: &crate::app::KnotView,
    target_state: &str,
    owner_kind: Option<&str>,
    reason: &str,
    dry_run: bool,
) -> String {
    let palette = ui::Palette::auto();
    let owner_suffix = owner_kind
        .map(|kind| format!(" (owner: {kind})"))
        .unwrap_or_default();
    let verb = if dry_run {
        "would roll back"
    } else {
        "rolled back"
    };
    format!(
        "{verb} {} -> {}{} ({reason})\n",
        palette.id(&knot_ref(knot)),
        palette.state(target_state),
        owner_suffix,
    )
}
