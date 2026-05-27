use super::operation_from_command;
use super::tests_lease_ext::parse;
use crate::write_queue::WriteOperation;

#[test]
fn operation_from_lease_create_includes_json() {
    let cli = parse(&["kno", "lease", "create", "--nickname", "sess", "--json"]);
    let op = operation_from_command(&cli.command);
    match op {
        Some(WriteOperation::LeaseCreate(c)) => {
            assert!(c.json, "json flag should be true");
        }
        other => panic!("expected LeaseCreate, got {:?}", other),
    }
}

#[test]
fn operation_from_new_includes_lease_id() {
    let cli = parse(&["kno", "new", "My title", "--lease", "lease-abc"]);
    let op = operation_from_command(&cli.command);
    match op {
        Some(WriteOperation::New(n)) => {
            assert_eq!(n.lease_id.as_deref(), Some("lease-abc"));
        }
        other => panic!("expected New, got {:?}", other),
    }
}

#[test]
fn operation_from_update_includes_lease_id() {
    let cli = parse(&["kno", "update", "knot-xyz", "--lease", "lease-abc"]);
    let op = operation_from_command(&cli.command);
    match op {
        Some(WriteOperation::Update(u)) => {
            assert_eq!(u.lease_id.as_deref(), Some("lease-abc"));
        }
        other => panic!("expected Update, got {:?}", other),
    }
}

#[test]
fn operation_from_claim_includes_lease_id() {
    let cli = parse(&["kno", "claim", "knot-xyz", "--lease", "lease-abc"]);
    let op = operation_from_command(&cli.command);
    match op {
        Some(WriteOperation::Claim(c)) => {
            assert_eq!(c.lease_id.as_deref(), Some("lease-abc"));
        }
        other => panic!("expected Claim, got {:?}", other),
    }
}

#[test]
fn operation_from_next_includes_lease_id() {
    let cli = parse(&[
        "kno",
        "next",
        "knot-xyz",
        "--expected-state",
        "implementation",
        "--lease",
        "lease-abc",
    ]);
    let op = operation_from_command(&cli.command);
    match op {
        Some(WriteOperation::Next(n)) => {
            assert_eq!(n.lease_id.as_deref(), Some("lease-abc"));
        }
        other => panic!("expected Next, got {:?}", other),
    }
}
