//! In-process MCP wire-protocol session test, split for size.

use super::tests::server_with_fixture;

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
