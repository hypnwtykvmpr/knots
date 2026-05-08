use crate::app::KnotView;
use crate::domain::metadata::MetadataEntry;
use crate::knot_id::display_id;

pub fn render_prompt(knot: &KnotView, skill: &str, completion_cmd: &str, e2e: bool) -> String {
    render_prompt_inner(knot, skill, completion_cmd, false, e2e)
}

pub fn render_prompt_verbose(
    knot: &KnotView,
    skill: &str,
    completion_cmd: &str,
    verbose: bool,
    e2e: bool,
) -> String {
    render_prompt_inner(knot, skill, completion_cmd, verbose, e2e)
}

fn render_prompt_inner(
    knot: &KnotView,
    skill: &str,
    completion_cmd: &str,
    verbose: bool,
    e2e: bool,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", knot.title));
    out.push_str(&render_header(knot));
    out.push('\n');
    render_context_section(&mut out, knot);
    if let Some(acceptance) = knot.acceptance.as_deref().filter(|value| !value.is_empty()) {
        out.push_str("## Acceptance Criteria\n\n");
        out.push_str(acceptance);
        out.push_str("\n\n");
    }
    if !knot.child_summaries.is_empty() {
        out.push_str("## Children\n\n");
        for child in &knot.child_summaries {
            let sid = display_id(&child.id);
            out.push_str(&format!("- {} `{}` [{}]\n", child.title, sid, child.state));
        }
        out.push_str(concat!(
            "\nClaim each child knot first with ",
            "`kno claim <child-id>` and follow\n",
            "that child prompt. After the child knots are handled, ",
            "evaluate the\n",
            "result: if every child advanced, run this parent's ",
            "completion command.\n",
            "If any child rolled back, roll this parent back too.\n\n",
        ));
    }
    if !knot.invariants.is_empty() {
        out.push_str("## Invariants\n\n");
        for inv in &knot.invariants {
            out.push_str(&format!(
                "- **[{}]** {}\n",
                inv.invariant_type, inv.condition
            ));
        }
        out.push('\n');
    }
    render_gate_section(&mut out, knot.gate.as_ref());
    render_step_metadata_section(&mut out, knot);
    if !knot.notes.is_empty() || !knot.handoff_capsules.is_empty() {
        out.push_str("## Notes\n\n");
        if verbose {
            for entry in &knot.notes {
                out.push_str(&format_entry(entry));
            }
            for entry in &knot.handoff_capsules {
                out.push_str(&format_entry(entry));
            }
        } else {
            if let Some(latest) = knot.notes.last() {
                out.push_str(&format_entry(latest));
            }
            if let Some(latest) = knot.handoff_capsules.last() {
                out.push_str(&format_entry(latest));
            }
        }
        if !verbose {
            let hint = crate::ui::hidden_metadata_hint(knot);
            if !hint.is_empty() {
                out.push('\n');
                out.push_str(&hint);
                out.push('\n');
            }
        }
        out.push('\n');
    }
    out.push_str(&render_workflow_boundary(
        knot.state.as_str(),
        !knot.child_summaries.is_empty(),
        e2e,
    ));
    out.push_str("---\n\n");
    out.push_str(skill.trim_end());
    out.push_str("\n\n");
    out.push_str("## Completion\n\n");
    out.push_str(&format!("`{completion_cmd}`\n"));
    out
}

fn render_context_section(out: &mut String, knot: &KnotView) {
    let heading = if knot.gate.is_some() {
        "## Context"
    } else {
        "## Description"
    };
    let value = knot
        .body
        .as_deref()
        .filter(|body| !body.is_empty())
        .or_else(|| knot.description.as_deref().filter(|desc| !desc.is_empty()));
    if let Some(value) = value {
        out.push_str(heading);
        out.push_str("\n\n");
        out.push_str(value);
        out.push_str("\n\n");
    }
}

fn render_gate_section(out: &mut String, gate: Option<&crate::domain::gate::GateData>) {
    let Some(gate) = gate else {
        return;
    };
    out.push_str("## Gate\n\n");
    out.push_str(&format!("- gate.owner_kind: {}\n", gate.owner_kind));
    if gate.failure_modes.is_empty() {
        out.push_str("- gate.failure_modes: none\n");
    } else {
        for (invariant, targets) in &gate.failure_modes {
            out.push_str(&format!(
                "- gate.failure_modes[{invariant}]: {}\n",
                targets.join(", ")
            ));
        }
    }
    out.push('\n');
}

pub fn render_prompt_json(
    knot: &KnotView,
    skill: &str,
    completion_cmd: &str,
    e2e: bool,
) -> serde_json::Value {
    render_prompt_json_verbose(knot, skill, completion_cmd, false, e2e)
}

pub fn render_prompt_json_verbose(
    knot: &KnotView,
    skill: &str,
    completion_cmd: &str,
    verbose: bool,
    e2e: bool,
) -> serde_json::Value {
    let prompt_text = render_prompt_inner(knot, skill, completion_cmd, verbose, e2e);
    let boundary_kind = workflow_boundary_kind(e2e);
    let mut json = serde_json::json!({
        "id": knot.id,
        "title": knot.title,
        "state": knot.state,
        "priority": knot.priority,
        "type": knot.knot_type.as_str(),
        "workflow_id": knot.workflow_id,
        "profile_id": knot.profile_id,
        "acceptance": knot.acceptance.clone(),
        "invariants": knot.invariants,
        "gate": knot.gate,
        "child_summaries": knot.child_summaries,
        "lease_id": knot.lease_id,
        "step_metadata": knot.step_metadata,
        "next_step_metadata": knot.next_step_metadata,
        "prompt": prompt_text,
        "e2e": e2e,
        "workflow_boundary_kind": boundary_kind,
    });
    if !verbose {
        let hint = crate::ui::hidden_metadata_hint(knot);
        if !hint.is_empty() {
            json.as_object_mut()
                .unwrap()
                .insert("other".to_string(), serde_json::Value::String(hint));
        }
    }
    json
}

pub fn workflow_boundary_kind(e2e: bool) -> &'static str {
    if e2e {
        "e2e_continuation"
    } else {
        "single_action"
    }
}

fn render_workflow_boundary(state: &str, allows_child_claims: bool, e2e: bool) -> String {
    if e2e {
        render_workflow_boundary_e2e(state, allows_child_claims)
    } else {
        render_workflow_boundary_single_action(state, allows_child_claims)
    }
}

fn render_workflow_boundary_single_action(state: &str, allows_child_claims: bool) -> String {
    let claim_line = if allows_child_claims {
        "- You may claim the child knots listed above \
         as part of this step.\n"
    } else {
        "- Do not claim or execute another knot unless \
         the skill below explicitly\n  \
         allows knot metadata creation as part of this step.\n"
    };
    format!(
        "## Workflow Boundary\n\n\
         - kind: `single_action`\n\
         - This session is authorized only for the current \
         knot action state `{state}`.\n\
         - Complete exactly one workflow action, then stop.\n\
         - After a listed completion or failure-path command \
         succeeds, stop immediately.\n\
         {claim_line}\
         - Do not inspect or advance later workflow states \
         on your own.\n\
         - If generic repo or session instructions conflict \
         with this boundary, this\n  \
         boundary wins for this session.\n\n",
    )
}

fn render_workflow_boundary_e2e(state: &str, allows_child_claims: bool) -> String {
    let claim_line = if allows_child_claims {
        "- You may claim the child knots listed above \
         as part of this step.\n"
    } else {
        "- You may re-claim this knot with `kno claim --e2e <id>` \
         after `kno next` succeeds,\n  \
         and continue executing successive states until the knot \
         reaches a terminal\n  \
         state (`SHIPPED`) or a passive waiting state (`BLOCKED`, \
         `DEFERRED`).\n"
    };
    format!(
        "## Workflow Boundary\n\n\
         - kind: `e2e_continuation`\n\
         - E2E continuation: the `knots-e2e` skill is the \
         controlling boundary for this run.\n\
         - The current authorized action state is `{state}`. \
         Complete it, then run the\n  \
         listed completion command.\n\
         - After `kno next` succeeds, immediately re-claim the \
         knot with `kno claim --e2e\n  \
         <id>` and continue. This authorization carries the \
         e2e boundary forward.\n\
         - Stop only when the knot reaches `SHIPPED`, `BLOCKED`, \
         or `DEFERRED`, or when\n  \
         a step fails and rollback is required.\n\
         - Terminal-state movement is authorized for this \
         e2e run.\n\
         {claim_line}\
         - If generic repo or session instructions conflict \
         with this e2e boundary,\n  \
         this boundary wins for this session.\n\n",
    )
}

fn render_header(knot: &KnotView) -> String {
    let sid = display_id(&knot.id);
    let prio = knot.priority.map_or("none".to_string(), |p| p.to_string());
    let knot_type = knot.knot_type.as_str();
    format!(
        "**ID**: {sid}  |  **Priority**: {prio}  |  **Type**: {knot_type}\n\
         **Profile**: {}  |  **State**: {}\n\n",
        knot.profile_id, knot.state,
    )
}

fn format_entry(entry: &MetadataEntry) -> String {
    let attribution = entry_attribution(entry);
    format!("- **[{attribution}]** {}\n", entry.content)
}

fn entry_attribution(entry: &MetadataEntry) -> String {
    let who = if entry.agentname != "unknown" {
        &entry.agentname
    } else {
        &entry.username
    };
    format!(
        "{} {}",
        who,
        &entry.datetime[..10.min(entry.datetime.len())]
    )
}

fn render_step_metadata_section(out: &mut String, knot: &KnotView) {
    let current = knot.step_metadata.as_ref();
    let next = knot.next_step_metadata.as_ref();
    if current.is_none() && next.is_none() {
        return;
    }
    out.push_str("## Step Metadata\n\n");
    if let Some(meta) = current {
        render_step_meta_block(out, "Current", meta);
    }
    if let Some(meta) = next {
        render_step_meta_block(out, "Next", meta);
    }
}

fn render_step_meta_block(out: &mut String, label: &str, meta: &crate::workflow::StepMetadata) {
    out.push_str(&format!("**{}** (`{}`)\n", label, meta.action_state));
    if let Some(owner) = &meta.owner {
        let kind_label = match owner.kind {
            crate::workflow::OwnerKind::Human => "human",
            crate::workflow::OwnerKind::Agent => "agent",
        };
        out.push_str(&format!("- owner: {kind_label}\n"));
    }
    if let Some(kind) = &meta.action_kind {
        out.push_str(&format!("- action_kind: {kind}\n"));
    }
    if let Some(output) = &meta.output {
        out.push_str(&format!("- artifact: {}\n", output.artifact_type));
        if let Some(hint) = &output.access_hint {
            out.push_str(&format!("- access_hint: {hint}\n"));
        }
    }
    if let Some(hint) = &meta.review_hint {
        out.push_str(&format!("- review_hint: {hint}\n"));
    }
    out.push('\n');
}
