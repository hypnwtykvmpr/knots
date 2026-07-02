use super::*;
use axum::body::Body;
use axum::http::HeaderValue;
use rmcp::ServerHandler;

#[test]
fn tool_router_lists_core_tools() {
    let server = KnoMcp::new(ServerConfig {
        repo: PathBuf::from("/tmp/repo"),
        kno_bin: PathBuf::from("/tmp/kno"),
        lease_timeout_seconds: 600,
    });
    let tools = server.tool_router.list_all();
    let names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert!(names.contains(&"knots_list"));
    assert!(names.contains(&"knots_claim"));
    assert!(names.contains(&"knots_next"));
    assert!(names.contains(&"knots_sync"));
    assert!(names.len() >= 7);
}

#[test]
fn authorization_checks_bearer_header() {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer secret"));
    assert!(authorized(&headers, "secret"));
    assert!(!authorized(&headers, "wrong"));
}

#[test]
fn accept_header_is_supplied_when_curl_omits_it() {
    let mut request = Request::builder().body(Body::empty()).expect("request");
    ensure_mcp_accept_header(&mut request);
    assert_eq!(
        request.headers().get(header::ACCEPT),
        Some(&HeaderValue::from_static(
            "application/json, text/event-stream"
        ))
    );

    request
        .headers_mut()
        .insert(header::ACCEPT, HeaderValue::from_static("*/*"));
    ensure_mcp_accept_header(&mut request);
    assert_eq!(
        request.headers().get(header::ACCEPT),
        Some(&HeaderValue::from_static(
            "application/json, text/event-stream"
        ))
    );
}

#[test]
fn content_type_is_supplied_when_curl_uses_form_default() {
    let mut request = Request::builder().body(Body::empty()).expect("request");
    ensure_mcp_content_type_header(&mut request);
    assert_eq!(
        request.headers().get(header::CONTENT_TYPE),
        Some(&HeaderValue::from_static("application/json"))
    );

    request.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    ensure_mcp_content_type_header(&mut request);
    assert_eq!(
        request.headers().get(header::CONTENT_TYPE),
        Some(&HeaderValue::from_static("application/json"))
    );
}

#[test]
fn created_id_reads_structured_content() {
    let result = CallToolResult::structured(serde_json::json!({ "id": "k1" }));
    assert_eq!(created_id(&result), Some("k1".to_string()));

    let result = CallToolResult::structured(serde_json::json!({ "data": [] }));
    assert_eq!(created_id(&result), None);
}

#[tokio::test]
async fn server_tools_route_to_kno_runner() {
    let server = server_with_fixture();
    let mut client = Implementation::new("client", "1.0");
    client.title = Some("provider".to_string());
    server
        .lease_registry
        .ensure_active(&server.runner, &server.session_key, &client, 60)
        .expect("lease");

    let list = server
        .knots_list(Parameters(ListArgs {
            state: Some("ready".to_string()),
            tag: None,
            knot_type: None,
            limit: Some(1),
            offset: Some(0),
        }))
        .await;
    assert_eq!(list.structured_content.unwrap()["total"], 1);

    assert_eq!(
        server
            .knots_show(Parameters(IdArgs {
                id: "k1".to_string()
            }))
            .await
            .structured_content
            .unwrap()["id"],
        "k1"
    );
    assert_eq!(
        server.knots_poll().await.structured_content.unwrap()["id"],
        "k1"
    );
    assert_eq!(
        server
            .knots_create(Parameters(CreateArgs {
                title: "New".to_string(),
                description: Some("Desc".to_string()),
                acceptance: Some("Done".to_string()),
                knot_type: Some("work".to_string()),
                priority: Some(3),
            }))
            .await
            .structured_content
            .unwrap()["priority"],
        3
    );
    assert_eq!(
        server
            .knots_update(Parameters(UpdateArgs {
                id: "k1".to_string(),
                title: Some("Updated".to_string()),
                description: None,
                acceptance: None,
                priority: None,
                state: None,
            }))
            .await
            .structured_content
            .unwrap()["title"],
        "updated"
    );
    let lease_id = server
        .lease_registry
        .get(&server.session_key)
        .expect("initialized session should have lease");
    assert_leased_workflow_tools_route(&server, lease_id);
    assert_eq!(
        server.knots_sync().await.structured_content.unwrap()["status"],
        "deferred"
    );
}

fn assert_leased_workflow_tools_route(server: &KnoMcp, lease_id: String) {
    assert_eq!(
        server
            .run_claim_tool(
                ClaimArgs {
                    id: "k1".to_string(),
                    e2e: Some(false),
                },
                Some(lease_id.clone()),
            )
            .structured_content
            .unwrap()["workflow_boundary_kind"],
        "single_action"
    );
    assert_eq!(
        server
            .run_leased_id_tool(
                "next",
                IdArgs {
                    id: "k1".to_string()
                },
                Some(lease_id.clone()),
            )
            .structured_content
            .unwrap()["state"],
        "ready_for_review"
    );
    let rollback = server
        .run_leased_id_tool(
            "rollback",
            IdArgs {
                id: "k1".to_string(),
            },
            Some(lease_id),
        )
        .structured_content
        .unwrap();
    assert_eq!(rollback["target_state"], "ready_for_implementation");
    assert_eq!(rollback["lease_present"], true);
}

#[tokio::test]
async fn create_without_priority_returns_created_result() {
    let server = server_with_fixture();
    let result = server
        .knots_create(Parameters(CreateArgs {
            title: "New".to_string(),
            description: None,
            acceptance: None,
            knot_type: None,
            priority: None,
        }))
        .await;

    assert_eq!(result.structured_content.unwrap()["id"], "k-new");
}

#[test]
fn server_info_advertises_tools() {
    let info = server_with_fixture().get_info();
    assert_eq!(info.server_info.name, "kno-mcp");
    assert!(info.capabilities.tools.is_some());
}

#[test]
fn build_http_router_accepts_server_configuration() {
    let router = build_http_router(
        ServerConfig {
            repo: PathBuf::from("/tmp/repo"),
            kno_bin: PathBuf::from("/tmp/kno"),
            lease_timeout_seconds: 600,
        },
        "secret".to_string(),
        LeaseRegistry::default(),
    );

    drop(router);
}

#[test]
fn streamable_http_config_preserves_sessions_for_lease_lookup() {
    let config = streamable_http_config();
    assert!(config.stateful_mode);
    assert!(!config.json_response);
    assert!(config.sse_keep_alive.is_none());
    assert!(config.allowed_hosts.is_empty());
}

#[test]
fn authorize_mcp_request_normalizes_headers_and_rejects_missing_token() {
    let mut request = Request::builder()
        .header(header::AUTHORIZATION, "Bearer secret")
        .header(header::ACCEPT, "*/*")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::empty())
        .expect("request");

    assert!(authorize_mcp_request(&mut request, "secret"));
    assert_eq!(
        request.headers().get(header::ACCEPT),
        Some(&HeaderValue::from_static(
            "application/json, text/event-stream"
        ))
    );
    assert_eq!(
        request.headers().get(header::CONTENT_TYPE),
        Some(&HeaderValue::from_static("application/json"))
    );

    let mut rejected = Request::builder().body(Body::empty()).expect("request");
    assert!(!authorize_mcp_request(&mut rejected, "secret"));
}

#[tokio::test]
async fn mutating_tool_reports_spawn_error_when_push_also_fails() {
    let server = KnoMcp::new(ServerConfig {
        repo: PathBuf::from("/tmp/repo"),
        kno_bin: PathBuf::from("missing-kno-mcp-test-binary"),
        lease_timeout_seconds: 600,
    });

    let result = server
        .knots_update(Parameters(UpdateArgs {
            id: "k1".to_string(),
            title: Some("Updated".to_string()),
            description: None,
            acceptance: None,
            priority: None,
            state: None,
        }))
        .await;
    assert_eq!(result.is_error, Some(true));
}

#[test]
fn identical_clients_on_distinct_sessions_get_independent_leases() {
    let registry = LeaseRegistry::default();
    let first = server_with_registry(registry.clone());
    let second = server_with_registry(registry.clone());
    let mut client = Implementation::new("client", "1.0");
    client.title = Some("provider".to_string());

    assert_ne!(first.session_key, second.session_key);
    first
        .lease_registry
        .ensure_active(&first.runner, &first.session_key, &client, 60)
        .expect("first session lease");
    second
        .lease_registry
        .ensure_active(&second.runner, &second.session_key, &client, 60)
        .expect("second session lease");

    // Two sessions of the same client app must never share a lease entry.
    assert_eq!(registry.len(), 2);
}

#[test]
fn session_lease_revalidates_and_falls_back_unambiguously() {
    let registry = LeaseRegistry::default();
    let first = server_with_registry(registry.clone());
    let second = server_with_registry(registry);
    let first_client = Implementation::new("client", "1.0");
    let second_client = Implementation::new("other-client", "1.0");

    assert_eq!(first.session_lease(None), None);
    first
        .lease_registry
        .ensure_active(&first.runner, &first.session_key, &first_client, 60)
        .expect("first lease");
    assert_eq!(first.session_lease(None), Some("L1".to_string()));
    assert_eq!(
        first.session_lease(Some(&first_client)),
        Some("L1".to_string())
    );
    // A sibling session without its own lease falls back to the single
    // unambiguous lease.
    assert_eq!(second.session_lease(None), Some("L1".to_string()));

    second
        .lease_registry
        .ensure_active(&second.runner, &second.session_key, &second_client, 60)
        .expect("second lease");
    assert_eq!(second.session_lease(None), Some("L2".to_string()));
    assert_eq!(first.session_lease(None), Some("L1".to_string()));
}

#[tokio::test]
async fn run_blocking_reports_worker_panics_as_tool_errors() {
    let result = run_blocking(|| panic!("worker exploded")).await;
    assert_eq!(result.is_error, Some(true));
}

async fn wire_send<W: tokio::io::AsyncWrite + Unpin>(writer: &mut W, message: &str) {
    use tokio::io::AsyncWriteExt as _;

    writer
        .write_all(format!("{message}\n").as_bytes())
        .await
        .expect("request should send");
}

async fn wire_call<W, R>(
    writer: &mut W,
    lines: &mut tokio::io::Lines<tokio::io::BufReader<R>>,
    message: &str,
) -> String
where
    W: tokio::io::AsyncWrite + Unpin,
    R: tokio::io::AsyncRead + Unpin,
{
    wire_send(writer, message).await;
    tokio::time::timeout(std::time::Duration::from_secs(30), lines.next_line())
        .await
        .expect("response should arrive before timeout")
        .expect("response should read")
        .expect("response should arrive")
}

/// Drives a real MCP session over an in-process transport: initialize
/// creates the session lease, and claim/next/rollback resolve it through
/// the wire path (peer info -> session key -> lease revalidation).
#[tokio::test]
async fn mcp_wire_session_initializes_and_claims_with_lease() {
    use tokio::io::AsyncBufReadExt as _;

    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let server = server_with_fixture();
    let registry = server.lease_registry.clone();
    // serve() performs the initialize handshake itself, so it must run
    // concurrently with the client side below.
    let server_task = tokio::spawn(async move {
        let running = rmcp::ServiceExt::serve(server, tokio::io::split(server_io))
            .await
            .expect("server should start");
        let _ = running.waiting().await;
    });

    let (client_read, mut client_write) = tokio::io::split(client_io);
    let mut lines = tokio::io::BufReader::new(client_read).lines();

    let init = wire_call(
        &mut client_write,
        &mut lines,
        concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"#,
            r#""protocolVersion":"2025-03-26","capabilities":{},"#,
            r#""clientInfo":{"name":"test-client","version":"1.2.3"}}}"#,
        ),
    )
    .await;
    assert!(init.contains("serverInfo"), "handshake failed: {init}");
    assert_eq!(
        registry.len(),
        1,
        "initialize should create a session lease"
    );

    wire_send(
        &mut client_write,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    )
    .await;

    let claim = wire_call(
        &mut client_write,
        &mut lines,
        concat!(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"#,
            r#""name":"knots_claim","arguments":{"id":"k1"}}}"#,
        ),
    )
    .await;
    assert!(claim.contains("single_action"), "claim failed: {claim}");
    assert!(
        claim.contains(r#""lease_present":true"#),
        "session lease should flow into claim: {claim}"
    );

    let next = wire_call(
        &mut client_write,
        &mut lines,
        concat!(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"#,
            r#""name":"knots_next","arguments":{"id":"k1"}}}"#,
        ),
    )
    .await;
    assert!(next.contains("ready_for_review"), "next failed: {next}");

    let rollback = wire_call(
        &mut client_write,
        &mut lines,
        concat!(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"#,
            r#""name":"knots_rollback","arguments":{"id":"k1"}}}"#,
        ),
    )
    .await;
    assert!(
        rollback.contains("ready_for_implementation"),
        "rollback failed: {rollback}"
    );

    drop(client_write);
    drop(lines);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), server_task).await;

    // The transport is gone and rmcp has dropped the session's server, so
    // the session lease guard must have released the lease.
    assert_eq!(
        registry.len(),
        0,
        "session disconnect should terminate its lease"
    );
}

#[test]
fn session_lease_survives_refresh_failure() {
    let server = KnoMcp::new(ServerConfig {
        repo: PathBuf::from("/tmp/repo"),
        kno_bin: PathBuf::from("missing-kno-mcp-test-binary"),
        lease_timeout_seconds: 600,
    });
    let client = Implementation::new("client", "1.0");
    // Lease refresh fails (no kno binary); the lookup degrades gracefully
    // instead of erroring the tool call.
    assert_eq!(server.session_lease(Some(&client)), None);
}

#[test]
fn terminate_session_leases_empties_the_registry() {
    let server = server_with_fixture();
    let client = Implementation::new("client", "1.0");
    server
        .lease_registry
        .ensure_active(&server.runner, &server.session_key, &client, 60)
        .expect("lease");

    server.terminate_session_leases();

    assert_eq!(server.lease_registry.get(&server.session_key), None);
}

fn server_with_fixture() -> KnoMcp {
    server_with_registry(LeaseRegistry::default())
}

fn server_with_registry(lease_registry: LeaseRegistry) -> KnoMcp {
    let fixture = if cfg!(windows) {
        "tests/fixtures/kno-stub.ps1"
    } else {
        "tests/fixtures/kno-stub.sh"
    };
    KnoMcp::with_lease_registry(
        ServerConfig {
            repo: PathBuf::from("/tmp/repo"),
            kno_bin: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture),
            lease_timeout_seconds: 600,
        },
        lease_registry,
    )
}
