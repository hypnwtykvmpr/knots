//! Workflow-driven state helpers.
//!
//! The canonical list of states, terminal markers, and legal transitions all
//! live in `ProfileDefinition`. Callers pass the relevant profile at the point
//! of use. Alias resolution supports legacy inputs like `idea` or `work_item`
//! by consulting `ProfileDefinition::state_aliases`.

use crate::profile::ProfileDefinition;

/// Normalize a user-supplied state string: trim, lowercase, replace dashes.
pub fn normalize_state_input(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

/// Resolve a raw state string against a profile.
///
/// Returns `Some(canonical)` if the string matches a canonical state directly,
/// or matches an alias declared by the profile. Returns `None` otherwise.
#[cfg_attr(not(test), allow(dead_code))]
pub fn resolve_state<'a>(profile: &'a ProfileDefinition, raw: &str) -> Option<&'a str> {
    let normalized = normalize_state_input(raw);
    if let Some(direct) = profile
        .states
        .iter()
        .find(|state| state.as_str() == normalized)
    {
        return Some(direct.as_str());
    }
    let alias_target = profile.state_aliases.get(&normalized)?;
    profile
        .states
        .iter()
        .find(|state| state.as_str() == alias_target.as_str())
        .map(String::as_str)
}

/// Check whether a state is terminal in the supplied profile.
#[cfg_attr(not(test), allow(dead_code))]
pub fn is_terminal(profile: &ProfileDefinition, state: &str) -> bool {
    profile.terminal_states.iter().any(|s| s == state)
}

/// Check whether `from -> to` is a permitted transition in the supplied
/// profile. Identity transitions are always allowed.
#[cfg_attr(not(test), allow(dead_code))]
pub fn can_transition(profile: &ProfileDefinition, from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    profile
        .transitions
        .iter()
        .any(|t| t.from == from && t.to == to)
}

/// Ordinal rank of the state within the profile's declared ordering.
#[cfg_attr(not(test), allow(dead_code))]
pub fn rank(profile: &ProfileDefinition, state: &str) -> Option<usize> {
    profile.states.iter().position(|s| s == state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::ProfileRegistry;

    fn registry() -> ProfileRegistry {
        ProfileRegistry::load().expect("registry should load")
    }

    #[test]
    fn normalize_state_input_handles_whitespace_case_and_dashes() {
        assert_eq!(
            normalize_state_input("  Ready-For-Planning "),
            "ready_for_planning"
        );
    }

    #[test]
    fn resolve_state_matches_canonical_states_in_builtin_profiles() {
        let registry = registry();
        let profile = registry.require("autopilot").expect("autopilot profile");
        assert_eq!(
            resolve_state(profile, "ready_for_implementation"),
            Some("ready_for_implementation")
        );
        assert_eq!(
            resolve_state(profile, "implementation"),
            Some("implementation")
        );
    }

    #[test]
    fn resolve_state_resolves_legacy_aliases() {
        let registry = registry();
        let profile = registry.require("autopilot").expect("autopilot profile");
        let pairs = [
            ("idea", "ready_for_planning"),
            ("work_item", "ready_for_implementation"),
            ("implementing", "implementation"),
            ("implemented", "ready_for_implementation_review"),
            ("reviewing", "implementation_review"),
            ("approved", "ready_for_shipment"),
            ("shipping", "shipment"),
        ];
        for (alias, target) in pairs {
            assert_eq!(
                resolve_state(profile, alias),
                Some(target),
                "alias '{alias}' should resolve to '{target}'"
            );
        }
    }

    #[test]
    fn resolve_state_returns_none_for_unknown_inputs() {
        let registry = registry();
        let profile = registry.require("autopilot").expect("autopilot profile");
        assert!(resolve_state(profile, "not_a_state").is_none());
    }

    #[test]
    fn terminal_reporting_matches_profile_terminal_states() {
        let registry = registry();
        let profile = registry.require("autopilot").expect("autopilot profile");
        assert!(is_terminal(profile, "shipped"));
        assert!(is_terminal(profile, "abandoned"));
        assert!(!is_terminal(profile, "implementation"));
        assert!(!is_terminal(profile, "deferred"));
    }

    #[test]
    fn can_transition_allows_identity_and_declared_pairs() {
        let registry = registry();
        let profile = registry.require("autopilot").expect("autopilot profile");
        assert!(can_transition(profile, "implementation", "implementation"));
        assert!(can_transition(
            profile,
            "ready_for_implementation",
            "implementation"
        ));
    }

    #[test]
    fn can_transition_rejects_undeclared_pairs() {
        let registry = registry();
        let profile = registry.require("autopilot").expect("autopilot profile");
        assert!(!can_transition(profile, "ready_for_planning", "shipped"));
    }

    #[test]
    fn rank_returns_declared_position() {
        let registry = registry();
        let profile = registry.require("autopilot").expect("autopilot profile");
        let first = profile.states.first().expect("states non-empty").clone();
        assert_eq!(rank(profile, &first), Some(0));
        assert!(rank(profile, "not_a_state").is_none());
    }

    #[test]
    fn lease_profile_terminal_states_include_lease_terminated() {
        let registry = registry();
        let profile = registry.require("lease").expect("lease profile");
        assert!(is_terminal(profile, "lease_terminated"));
        assert!(!is_terminal(profile, "lease_ready"));
        assert!(!is_terminal(profile, "lease_active"));
    }

    #[test]
    fn resolve_state_works_with_custom_profile_states() {
        use crate::profile::{GateMode, ProfileDefinition, ProfileOwners, WorkflowTransition};
        use std::collections::BTreeMap;

        let profile = ProfileDefinition {
            id: "custom".to_string(),
            workflow_id: "custom_wf".to_string(),
            aliases: Vec::new(),
            description: None,
            planning_mode: GateMode::Skipped,
            implementation_review_mode: GateMode::Skipped,
            outputs: BTreeMap::new(),
            owners: ProfileOwners {
                states: BTreeMap::new(),
            },
            initial_state: "ready_for_orchestration".to_string(),
            states: vec![
                "ready_for_orchestration".to_string(),
                "orchestrating".to_string(),
                "shipped".to_string(),
            ],
            queue_states: vec!["ready_for_orchestration".to_string()],
            action_states: vec!["orchestrating".to_string()],
            queue_actions: BTreeMap::new(),
            action_kinds: BTreeMap::new(),
            escape_states: Vec::new(),
            terminal_states: vec!["shipped".to_string()],
            transitions: vec![
                WorkflowTransition {
                    from: "ready_for_orchestration".to_string(),
                    to: "orchestrating".to_string(),
                },
                WorkflowTransition {
                    from: "orchestrating".to_string(),
                    to: "shipped".to_string(),
                },
            ],
            action_prompts: BTreeMap::new(),
            prompt_acceptance: BTreeMap::new(),
            review_hints: BTreeMap::new(),
            state_aliases: BTreeMap::from([(
                "orchestrate".to_string(),
                "orchestrating".to_string(),
            )]),
        };

        assert_eq!(
            resolve_state(&profile, "ready_for_orchestration"),
            Some("ready_for_orchestration")
        );
        assert_eq!(
            resolve_state(&profile, "orchestrate"),
            Some("orchestrating")
        );
        assert!(is_terminal(&profile, "shipped"));
        assert!(!is_terminal(&profile, "orchestrating"));
        assert_eq!(rank(&profile, "orchestrating"), Some(1));
        assert!(can_transition(&profile, "orchestrating", "shipped"));
        assert!(!can_transition(
            &profile,
            "ready_for_orchestration",
            "shipped"
        ));
    }
}
