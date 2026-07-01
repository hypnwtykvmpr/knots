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
        .get_or_create(&server.runner, &client, 60)
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
    assert_eq!(
        server
            .knots_claim(Parameters(ClaimArgs {
                id: "k1".to_string(),
                e2e: Some(false),
            }))
            .await
            .structured_content
            .unwrap()["workflow_boundary_kind"],
        "single_action"
    );
    assert_eq!(
        server
            .knots_next(Parameters(IdArgs {
                id: "k1".to_string()
            }))
            .await
            .structured_content
            .unwrap()["state"],
        "ready_for_review"
    );
    assert_eq!(
        server
            .knots_rollback(Parameters(IdArgs {
                id: "k1".to_string()
            }))
            .await
            .structured_content
            .unwrap()["target_state"],
        "ready_for_implementation"
    );
    assert_eq!(
        server.knots_sync().await.structured_content.unwrap()["status"],
        "deferred"
    );
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
fn shared_registry_carries_lease_between_http_instances() {
    let registry = LeaseRegistry::default();
    let first = server_with_registry(registry.clone());
    let second = server_with_registry(registry);
    let mut client = Implementation::new("client", "1.0");
    client.title = Some("provider".to_string());

    first
        .lease_registry
        .get_or_create(&first.runner, &client, 60)
        .expect("lease");

    assert_eq!(second.lease_registry.current(), Some("L1".to_string()));
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
