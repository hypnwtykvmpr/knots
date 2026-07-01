use clap::Parser;

use super::operation_from_command;
use crate::write_queue::WriteOperation;

#[test]
fn operation_from_command_maps_quick_new_poll_claim_and_read_only_guards() {
    let quick = crate::cli::Cli::parse_from(["kno", "q", "Fast title", "-d", "details", "--json"]);
    match operation_from_command(&quick.command).expect("quick new should map") {
        WriteOperation::QuickNew(op) => {
            assert_eq!(op.title, "Fast title");
            assert_eq!(op.description.as_deref(), Some("details"));
            assert!(op.json);
        }
        other => panic!("unexpected: {other:?}"),
    }

    let poll = crate::cli::Cli::parse_from([
        "kno",
        "poll",
        "implementation",
        "--claim",
        "--owner",
        "agent",
        "--timeout-seconds",
        "5",
        "--e2e",
    ]);
    match operation_from_command(&poll.command).expect("poll claim should map") {
        WriteOperation::PollClaim(op) => {
            assert_eq!(op.stage.as_deref(), Some("implementation"));
            assert_eq!(op.owner.as_deref(), Some("agent"));
            assert_eq!(op.timeout_seconds, Some(5));
            assert!(op.e2e);
        }
        other => panic!("unexpected: {other:?}"),
    }

    for argv in [
        vec!["kno", "claim", "knots-1", "--peek"],
        vec!["kno", "poll"],
        vec!["kno", "edge", "list", "knots-1"],
        vec!["kno", "lease", "show", "lease-1"],
        vec!["kno", "lease", "list"],
    ] {
        let cli = crate::cli::Cli::parse_from(argv);
        assert!(operation_from_command(&cli.command).is_none());
    }
}

#[test]
fn operation_from_command_absolutizes_update_plan_file() {
    let cli = crate::cli::Cli::parse_from([
        "kno",
        "update",
        "knots-1",
        "--execution-plan-file",
        "plan.json",
    ]);
    match operation_from_command(&cli.command).expect("update should map") {
        WriteOperation::Update(op) => {
            let plan = op
                .execution_plan_file
                .expect("execution plan file should be mapped");
            assert!(std::path::Path::new(&plan).is_absolute());
            assert!(plan.ends_with("plan.json"));
        }
        other => panic!("unexpected: {other:?}"),
    }
}
