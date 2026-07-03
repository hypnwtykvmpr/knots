//! Serialization round-trips for queued write operations.

use super::*;

#[test]
fn lease_create_operation_serializes_round_trip() {
    let op = WriteOperation::LeaseCreate(LeaseCreateOperation {
        nickname: "test-session".to_string(),
        lease_type: "agent".to_string(),
        agent_type: Some("cli".to_string()),
        provider: Some("Anthropic".to_string()),
        agent_name: Some("claude".to_string()),
        model: Some("opus".to_string()),
        model_version: Some("4.6".to_string()),
        json: false,
        timeout_seconds: None,
    });
    let json = serde_json::to_string(&op).expect("should serialize");
    let parsed: WriteOperation = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(parsed, op);
}

#[test]
fn lease_terminate_operation_serializes_round_trip() {
    let op = WriteOperation::LeaseTerminate(LeaseTerminateOperation {
        id: "knot-abc123".to_string(),
    });
    let json = serde_json::to_string(&op).expect("should serialize");
    let parsed: WriteOperation = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(parsed, op);
}

#[test]
fn lease_create_operation_with_no_optional_fields() {
    let op = WriteOperation::LeaseCreate(LeaseCreateOperation {
        nickname: "manual-session".to_string(),
        lease_type: "manual".to_string(),
        agent_type: None,
        provider: None,
        agent_name: None,
        model: None,
        model_version: None,
        json: false,
        timeout_seconds: None,
    });
    let json = serde_json::to_string(&op).expect("should serialize");
    let parsed: WriteOperation = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(parsed, op);
}

#[test]
fn lease_extend_operation_serializes_round_trip() {
    let op = WriteOperation::LeaseExtend(LeaseExtendOperation {
        lease_id: "knot-lease-1".to_string(),
        timeout_seconds: Some(1200),
        json: true,
    });
    let json = serde_json::to_string(&op).expect("should serialize");
    let parsed: WriteOperation = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(parsed, op);
}

#[test]
fn rollback_operation_deserializes_without_lease_for_old_queued_requests() {
    let raw = serde_json::json!({
        "Rollback": {
            "id": "K-123",
            "dry_run": false,
            "actor_kind": null,
            "agent_name": null,
            "agent_model": null,
            "agent_version": null,
            "json": true
        }
    });
    let parsed: WriteOperation =
        serde_json::from_value(raw).expect("old rollback operation should deserialize");
    let WriteOperation::Rollback(operation) = parsed else {
        panic!("expected rollback operation");
    };
    assert_eq!(operation.lease_id, None);
    assert!(operation.json);
}
