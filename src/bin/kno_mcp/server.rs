use std::error::Error;
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::{extract::Request, response::Response, Router};
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{
    CallToolResult, Implementation, InitializeRequestParams, InitializeResult, ServerCapabilities,
    ServerInfo,
};
use rmcp::service::{MaybeSendFuture, RequestContext, RoleServer};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};

use crate::auth::bearer_token_matches;
use crate::runner::KnoRunner;
use crate::session::LeaseRegistry;
use crate::sync_loop::spawn_background_sync;
use crate::tools::{
    claim_argv, create_argv, id_argv, leased_id_argv, list_argv, update_argv, ClaimArgs,
    CreateArgs, IdArgs, ListArgs, UpdateArgs,
};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub repo: PathBuf,
    pub kno_bin: PathBuf,
    pub lease_timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub bind: String,
    pub token: String,
    pub sync_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct KnoMcp {
    runner: KnoRunner,
    lease_registry: LeaseRegistry,
    lease_timeout_seconds: u64,
    tool_router: ToolRouter<Self>,
}

impl KnoMcp {
    pub fn new(config: ServerConfig) -> Self {
        Self::with_lease_registry(config, LeaseRegistry::default())
    }

    fn with_lease_registry(config: ServerConfig, lease_registry: LeaseRegistry) -> Self {
        Self {
            runner: KnoRunner::new(config.kno_bin, config.repo),
            lease_registry,
            lease_timeout_seconds: config.lease_timeout_seconds,
            tool_router: Self::tool_router(),
        }
    }

    fn run_tool(&self, subcommand: &str, args: Vec<String>) -> CallToolResult {
        self.runner.run_tool(subcommand, &args)
    }

    fn run_mutating_tool(&self, subcommand: &str, args: Vec<String>) -> CallToolResult {
        let result = self.runner.run_tool(subcommand, &args);
        match self.runner.run_allowing_active_leases("push", &[]) {
            Ok(value) => eprintln!(
                "kno-mcp post-mutation push after {subcommand}: {}",
                push_detail(&value)
            ),
            Err(err) => eprintln!(
                "kno-mcp post-mutation push failed after {subcommand}: {}",
                err.stderr
            ),
        }
        result
    }

    fn lease_for_context(&self, context: &RequestContext<RoleServer>) -> Option<String> {
        self.lease_for_client(context.peer.peer_info().map(|info| &info.client_info))
    }

    fn lease_for_client(&self, client: Option<&Implementation>) -> Option<String> {
        client
            .and_then(|client| self.lease_registry.get(client))
            .or_else(|| self.lease_registry.single_lease())
    }

    fn run_claim_tool(&self, args: ClaimArgs, lease_id: Option<String>) -> CallToolResult {
        self.run_mutating_tool("claim", claim_argv(args, lease_id))
    }

    fn run_leased_id_tool(
        &self,
        subcommand: &str,
        args: IdArgs,
        lease_id: Option<String>,
    ) -> CallToolResult {
        self.run_mutating_tool(subcommand, leased_id_argv(args, lease_id))
    }
}

fn push_detail(value: &serde_json::Value) -> String {
    let copied = value
        .get("copied_files")
        .and_then(serde_json::Value::as_u64);
    let pushed = value.get("pushed").and_then(serde_json::Value::as_bool);
    format!("copied_files={copied:?} pushed={pushed:?}")
}

#[tool_router(router = tool_router)]
impl KnoMcp {
    #[tool(description = "List knots, optionally filtered by state, tag, type, or limit.")]
    pub async fn knots_list(&self, Parameters(args): Parameters<ListArgs>) -> CallToolResult {
        self.run_tool("ls", list_argv(args))
    }

    #[tool(description = "Show a single knot by id or alias.")]
    pub async fn knots_show(&self, Parameters(args): Parameters<IdArgs>) -> CallToolResult {
        self.run_tool("show", id_argv(args))
    }

    #[tool(description = "Poll for the highest-priority claimable knot.")]
    pub async fn knots_poll(&self) -> CallToolResult {
        self.run_tool("poll", Vec::new())
    }

    #[tool(description = "Create a new knot and return the created KnotView JSON.")]
    pub async fn knots_create(&self, Parameters(args): Parameters<CreateArgs>) -> CallToolResult {
        let priority = args.priority;
        let result = self.run_mutating_tool("new", create_argv(args));
        if let (Some(priority), Some(id)) = (priority, created_id(&result)) {
            let update_args = vec![id, "--priority".to_string(), priority.to_string()];
            return self.run_mutating_tool("update", update_args);
        }
        result
    }

    #[tool(description = "Update knot fields.")]
    pub async fn knots_update(&self, Parameters(args): Parameters<UpdateArgs>) -> CallToolResult {
        self.run_mutating_tool("update", update_argv(args))
    }

    #[tool(description = "Claim a knot, preserving the CLI claim-boundary contract.")]
    pub async fn knots_claim(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(args): Parameters<ClaimArgs>,
    ) -> CallToolResult {
        self.run_claim_tool(args, self.lease_for_context(&context))
    }

    #[tool(description = "Advance a claimed knot to its next workflow state.")]
    pub async fn knots_next(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(args): Parameters<IdArgs>,
    ) -> CallToolResult {
        self.run_leased_id_tool("next", args, self.lease_for_context(&context))
    }

    #[tool(description = "Roll back a knot from an action state to its prior ready state.")]
    pub async fn knots_rollback(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(args): Parameters<IdArgs>,
    ) -> CallToolResult {
        self.run_leased_id_tool("rollback", args, self.lease_for_context(&context))
    }

    #[tool(description = "Run Knots git sync and return the SyncOutcome JSON.")]
    pub async fn knots_sync(&self) -> CallToolResult {
        self.run_tool("sync", Vec::new())
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for KnoMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("kno-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions("Knots MCP server")
    }

    #[cfg(not(tarpaulin_include))]
    fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<InitializeResult, McpError>> + MaybeSendFuture + '_ {
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request.clone());
        }
        async move {
            self.lease_registry
                .get_or_create(
                    &self.runner,
                    &request.client_info,
                    self.lease_timeout_seconds,
                )
                .map_err(|err| McpError::internal_error(err, None))?;
            Ok(self.get_info())
        }
    }
}

#[cfg(not(tarpaulin_include))]
pub async fn serve_http(
    server_config: ServerConfig,
    http: HttpConfig,
) -> Result<(), Box<dyn Error>> {
    let HttpConfig {
        bind,
        token,
        sync_interval,
    } = http;
    let runner = KnoRunner::new(server_config.kno_bin.clone(), server_config.repo.clone());
    spawn_background_sync(runner, sync_interval);
    let router = build_http_router(server_config, token, LeaseRegistry::default());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

#[cfg(not(tarpaulin_include))]
fn build_http_router(
    server_config: ServerConfig,
    token: String,
    lease_registry: LeaseRegistry,
) -> Router {
    let service: StreamableHttpService<KnoMcp, LocalSessionManager> = StreamableHttpService::new(
        move || {
            Ok(KnoMcp::with_lease_registry(
                server_config.clone(),
                lease_registry.clone(),
            ))
        },
        Default::default(),
        streamable_http_config(),
    );
    Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn(move |request, next| {
            auth_middleware(request, next, token.clone())
        }))
}

#[cfg(not(tarpaulin_include))]
fn streamable_http_config() -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::default()
        .with_stateful_mode(true)
        .with_sse_keep_alive(None)
        .disable_allowed_hosts()
}

#[cfg(not(tarpaulin_include))]
async fn auth_middleware(
    mut request: Request,
    next: Next,
    token: String,
) -> Result<Response, StatusCode> {
    if authorize_mcp_request(&mut request, &token) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn authorize_mcp_request(request: &mut Request, token: &str) -> bool {
    if !authorized(request.headers(), token) {
        return false;
    }
    ensure_mcp_accept_header(request);
    ensure_mcp_content_type_header(request);
    true
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| bearer_token_matches(Some(value), token))
}

fn ensure_mcp_accept_header(request: &mut Request) {
    let accept_is_missing_or_wildcard = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim() == "*/*")
        .unwrap_or(true);
    if accept_is_missing_or_wildcard {
        request.headers_mut().insert(
            header::ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
    }
}

fn ensure_mcp_content_type_header(request: &mut Request) {
    let content_type_is_missing_or_curl_default = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.starts_with("application/x-www-form-urlencoded"))
        .unwrap_or(true);
    if content_type_is_missing_or_curl_default {
        request.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }
}

fn created_id(result: &CallToolResult) -> Option<String> {
    result
        .structured_content
        .as_ref()
        .and_then(|value| value.get("id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
