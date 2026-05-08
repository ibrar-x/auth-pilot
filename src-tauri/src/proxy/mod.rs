//! Local AuthPilot proxy foundations.
//!
//! Phase 1 is intentionally limited to localhost CLI HTTP forwarding. macOS
//! system proxy, Keychain trust, CONNECT/TLS interception, launchd, and shell
//! mutation are separate consent-gated phases.

use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Method, Request, StatusCode, Uri},
    response::Response,
    Router,
};
use hyper::upgrade;
use hyper_util::rt::TokioIo;
use reqwest::header::{HeaderName, HeaderValue, AUTHORIZATION, CONTENT_LENGTH, HOST};
use tauri::{AppHandle, Emitter};
use tokio::io::copy_bidirectional;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::auth::storage;
use crate::types::{AppSettings, AuthData, StoredAccount};

pub const DEFAULT_PROXY_PORT: u16 = 18080;
const PROXY_LOCKFILE_NAME: &str = ".authpilot-proxy-active";
const OPENAI_API_HOST: &str = "api.openai.com";

pub type SharedProxyRuntime = Arc<Mutex<Option<ProxyRuntime>>>;

#[derive(Debug, Clone)]
pub struct ProxyStartOptions {
    pub port: u16,
    pub app_handle: Option<AppHandle>,
}

impl Default for ProxyStartOptions {
    fn default() -> Self {
        Self {
            port: DEFAULT_PROXY_PORT,
            app_handle: None,
        }
    }
}

#[derive(Debug)]
pub struct ProxyRuntime {
    pub addr: SocketAddr,
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl ProxyRuntime {
    pub async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

pub fn runtime_state() -> SharedProxyRuntime {
    Arc::new(Mutex::new(None))
}

pub async fn sync_runtime(
    runtime: SharedProxyRuntime,
    settings: &AppSettings,
    app_handle: Option<AppHandle>,
) -> Result<()> {
    if !settings.proxy_mode_enabled {
        stop_runtime(runtime).await;
        return Ok(());
    }

    let mut guard = runtime.lock().await;
    if guard
        .as_ref()
        .map(|existing| existing.addr.port() == settings.proxy_port)
        .unwrap_or(false)
    {
        return Ok(());
    }

    if let Some(existing) = guard.take() {
        existing.stop().await;
        clear_lockfile()?;
    }

    check_and_restore_after_crash()?;
    let started = start(ProxyStartOptions {
        port: settings.proxy_port,
        app_handle,
    })
    .await?;
    write_lockfile(started.addr)?;
    *guard = Some(started);
    Ok(())
}

pub async fn stop_runtime(runtime: SharedProxyRuntime) {
    let mut guard = runtime.lock().await;
    if let Some(existing) = guard.take() {
        existing.stop().await;
    }
    if let Err(err) = clear_lockfile() {
        tracing::warn!("[proxy] failed to clear lockfile: {err}");
    }
}

#[derive(Clone)]
struct ProxyState {
    client: reqwest::Client,
    app_handle: Option<AppHandle>,
}

pub async fn start(options: ProxyStartOptions) -> Result<ProxyRuntime> {
    let app_handle = options.app_handle.clone();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), options.port);
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind AuthPilot proxy on {addr}"))?;
    let actual_addr = listener
        .local_addr()
        .context("failed to read AuthPilot proxy local address")?;
    let state = ProxyState {
        client: reqwest::Client::new(),
        app_handle,
    };
    let app = Router::new().fallback(handle_cli_request).with_state(state);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        let _ = shutdown_rx.await;
    });
    let task = tokio::spawn(async move {
        if let Err(err) = server.await {
            tracing::error!("[proxy] server stopped with error: {err}");
        }
    });

    tracing::info!("[proxy] CLI HTTP proxy listening on {actual_addr}");
    Ok(ProxyRuntime {
        addr: actual_addr,
        shutdown: shutdown_tx,
        task,
    })
}

pub async fn is_local_proxy_listening(port: u16) -> bool {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_millis(250),
            TcpStream::connect(addr)
        )
        .await,
        Ok(Ok(_))
    )
}

pub fn check_and_restore_after_crash() -> Result<bool> {
    let path = lockfile_path()?;
    if !path.exists() {
        return Ok(false);
    }

    tracing::warn!("[proxy] stale proxy lockfile detected; clearing phase-1 proxy state");
    fs::remove_file(&path)
        .with_context(|| format!("failed to remove stale proxy lockfile {}", path.display()))?;
    Ok(true)
}

pub fn write_lockfile(addr: SocketAddr) -> Result<()> {
    let path = lockfile_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create proxy lockfile directory {}",
                parent.display()
            )
        })?;
    }

    fs::write(&path, format!("pid={}\naddr={addr}\n", std::process::id()))
        .with_context(|| format!("failed to write proxy lockfile {}", path.display()))?;
    Ok(())
}

pub fn clear_lockfile() -> Result<()> {
    let path = lockfile_path()?;
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove proxy lockfile {}", path.display()))?;
    }
    Ok(())
}

fn lockfile_path() -> Result<PathBuf> {
    Ok(lockfile_path_for_config_dir(&storage::get_config_dir()?))
}

fn lockfile_path_for_config_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(PROXY_LOCKFILE_NAME)
}

pub fn should_inject_auth_host(host: &str) -> bool {
    host.trim_end_matches('.')
        .eq_ignore_ascii_case(OPENAI_API_HOST)
}

pub fn bearer_token_for_account(account: &StoredAccount) -> Option<String> {
    let raw = match &account.auth_data {
        AuthData::ApiKey { key } => key,
        AuthData::ChatGPT { access_token, .. } => access_token,
    }
    .trim();

    (!raw.is_empty()).then(|| format!("Bearer {raw}"))
}

pub fn active_bearer_token() -> Result<Option<String>> {
    Ok(storage::get_active_account()?.and_then(|account| bearer_token_for_account(&account)))
}

pub fn apply_auth_headers(host: &str, headers: &mut HeaderMap, bearer_token: Option<&str>) {
    if !should_inject_auth_host(host) {
        return;
    }

    headers.remove(AUTHORIZATION);
    if let Some(token) = bearer_token.and_then(valid_header_value) {
        headers.insert(AUTHORIZATION, token);
    }
}

fn valid_header_value(value: &str) -> Option<HeaderValue> {
    HeaderValue::from_str(value).ok()
}

async fn handle_cli_request(
    State(state): State<ProxyState>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    if req.method() == Method::CONNECT {
        return match handle_connect_tunnel(req).await {
            Ok(response) => Ok(response),
            Err(err) => {
                tracing::warn!("[proxy] CONNECT tunnel failed: {err}");
                Err(StatusCode::BAD_GATEWAY)
            }
        };
    }

    match forward_cli_request(state, req).await {
        Ok(response) => Ok(response),
        Err(err) => {
            tracing::warn!("[proxy] CLI request failed: {err}");
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

async fn handle_connect_tunnel(req: Request<Body>) -> Result<Response<Body>> {
    let target = connect_target_from_uri(req.uri())?;
    tokio::spawn(async move {
        match upgrade::on(req).await {
            Ok(upgraded) => {
                let mut upgraded = TokioIo::new(upgraded);
                match TcpStream::connect(&target).await {
                    Ok(mut server) => {
                        if let Err(err) = copy_bidirectional(&mut upgraded, &mut server).await {
                            tracing::warn!(
                                "[proxy] CONNECT tunnel copy failed for {target}: {err}"
                            );
                        }
                    }
                    Err(err) => {
                        tracing::warn!("[proxy] CONNECT upstream failed for {target}: {err}");
                    }
                }
            }
            Err(err) => {
                tracing::warn!("[proxy] CONNECT upgrade failed for {target}: {err}");
            }
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::empty())
        .context("failed to build CONNECT response")
}

async fn forward_cli_request(state: ProxyState, req: Request<Body>) -> Result<Response<Body>> {
    let (parts, body) = req.into_parts();
    let uri = upstream_uri_for_cli_request(&parts.uri)?;
    let bearer_token = tokio::task::spawn_blocking(active_bearer_token)
        .await
        .context("active token lookup task failed")??;

    let mut headers = parts.headers;
    strip_hop_by_hop_headers(&mut headers);
    apply_auth_headers(OPENAI_API_HOST, &mut headers, bearer_token.as_deref());

    let body_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .context("failed to read CLI proxy request body")?;

    let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())
        .context("unsupported HTTP method")?;
    let mut builder = state.client.request(method, uri);
    for (name, value) in headers.iter() {
        builder = builder.header(name, value);
    }

    let upstream = builder
        .body(body_bytes)
        .send()
        .await
        .context("OpenAI upstream request failed")?;
    response_from_reqwest(upstream, state.app_handle.as_ref()).await
}

fn upstream_uri_for_cli_request(uri: &Uri) -> Result<String> {
    if let Some(host) = uri.host() {
        if !should_inject_auth_host(host) {
            anyhow::bail!("proxy only forwards OpenAI CLI requests");
        }
        return Ok(uri.to_string());
    }

    let path = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    Ok(format!("https://{OPENAI_API_HOST}{path}"))
}

fn connect_target_from_uri(uri: &Uri) -> Result<String> {
    let authority = uri
        .authority()
        .map(|authority| authority.as_str())
        .filter(|authority| !authority.is_empty())
        .context("CONNECT request missing authority")?;

    if authority.rsplit_once(':').is_some() {
        Ok(authority.to_string())
    } else {
        Ok(format!("{authority}:443"))
    }
}

fn strip_hop_by_hop_headers(headers: &mut HeaderMap) {
    headers.remove(HOST);
    headers.remove(CONTENT_LENGTH);
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(HeaderName::from_static(name));
    }
}

async fn response_from_reqwest(
    upstream: reqwest::Response,
    app_handle: Option<&AppHandle>,
) -> Result<Response<Body>> {
    let status = StatusCode::from_u16(upstream.status().as_u16())?;
    let upstream_headers = upstream.headers().clone();

    if should_warn_openai_unauthorized(status) {
        tracing::warn!(
            "[proxy] OpenAI upstream returned 401 Unauthorized; active account token may be expired"
        );
        if let Some(app_handle) = app_handle {
            let active_account_id = tokio::task::spawn_blocking(|| {
                storage::get_active_account()
                    .ok()
                    .flatten()
                    .map(|account| account.id)
            })
            .await
            .ok()
            .flatten();
            let _ = app_handle.emit("token-expired", active_account_id);
        }
    }

    let mut response = Response::builder().status(status);
    for (name, value) in upstream_headers.iter() {
        if should_copy_response_header(name) {
            response = response.header(name, value);
        }
    }
    response
        .body(Body::from_stream(upstream.bytes_stream()))
        .context("failed to build proxy response")
}

pub fn should_warn_openai_unauthorized(status: StatusCode) -> bool {
    status == StatusCode::UNAUTHORIZED
}

fn should_copy_response_header(name: &HeaderName) -> bool {
    !matches!(
        name.as_str(),
        "connection"
            | "content-length"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn chatgpt_account(access_token: &str) -> StoredAccount {
        StoredAccount::new_chatgpt(
            "chatgpt".to_string(),
            Some("user@example.com".to_string()),
            Some("plus".to_string()),
            None,
            "id-token".to_string(),
            access_token.to_string(),
            "refresh-token".to_string(),
            Some("account-id".to_string()),
        )
    }

    #[test]
    fn injects_only_exact_openai_api_host() {
        assert!(should_inject_auth_host("api.openai.com"));
        assert!(should_inject_auth_host("API.OPENAI.COM"));
        assert!(should_inject_auth_host("api.openai.com."));
        assert!(!should_inject_auth_host("sub.api.openai.com"));
        assert!(!should_inject_auth_host("api.openai.com.evil.test"));
        assert!(!should_inject_auth_host("openai.com"));
    }

    #[test]
    fn maps_api_key_account_to_bearer_token() {
        let account = StoredAccount::new_api_key("api".to_string(), " sk-test ".to_string());
        assert_eq!(
            bearer_token_for_account(&account),
            Some("Bearer sk-test".to_string())
        );
    }

    #[test]
    fn maps_chatgpt_account_to_access_token() {
        assert_eq!(
            bearer_token_for_account(&chatgpt_account("access-token")),
            Some("Bearer access-token".to_string())
        );
    }

    #[test]
    fn empty_tokens_do_not_create_authorization_values() {
        let api = StoredAccount::new_api_key("api".to_string(), "   ".to_string());
        assert_eq!(bearer_token_for_account(&api), None);
        assert_eq!(bearer_token_for_account(&chatgpt_account("")), None);
    }

    #[test]
    fn apply_auth_headers_replaces_openai_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer old"));

        apply_auth_headers("api.openai.com", &mut headers, Some("Bearer new"));

        assert_eq!(
            headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer new")
        );
    }

    #[test]
    fn apply_auth_headers_preserves_non_openai_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer original"));

        apply_auth_headers("example.com", &mut headers, Some("Bearer new"));

        assert_eq!(
            headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer original")
        );
    }

    #[test]
    fn openai_unauthorized_warning_is_only_for_401() {
        assert!(should_warn_openai_unauthorized(StatusCode::UNAUTHORIZED));
        assert!(!should_warn_openai_unauthorized(StatusCode::OK));
        assert!(!should_warn_openai_unauthorized(
            StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(!should_warn_openai_unauthorized(
            StatusCode::INTERNAL_SERVER_ERROR
        ));
    }

    #[test]
    fn response_header_copy_allows_end_to_end_metadata() {
        assert!(should_copy_response_header(&HeaderName::from_static(
            "content-type"
        )));
        assert!(should_copy_response_header(&HeaderName::from_static(
            "openai-request-id"
        )));
        assert!(should_copy_response_header(&HeaderName::from_static(
            "x-ratelimit-remaining-requests"
        )));
    }

    #[test]
    fn response_header_copy_rejects_framing_and_hop_by_hop_headers() {
        for name in [
            "connection",
            "content-length",
            "keep-alive",
            "proxy-authenticate",
            "proxy-authorization",
            "te",
            "trailer",
            "transfer-encoding",
            "upgrade",
        ] {
            assert!(
                !should_copy_response_header(&HeaderName::from_static(name)),
                "{name} should not be copied"
            );
        }
    }

    #[test]
    fn cli_relative_uri_maps_to_openai_upstream() {
        let uri: Uri = "/v1/responses?stream=true".parse().unwrap();
        assert_eq!(
            upstream_uri_for_cli_request(&uri).unwrap(),
            "https://api.openai.com/v1/responses?stream=true"
        );
    }

    #[test]
    fn absolute_openai_uri_is_allowed() {
        let uri: Uri = "https://api.openai.com/v1/models".parse().unwrap();
        assert_eq!(
            upstream_uri_for_cli_request(&uri).unwrap(),
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn absolute_non_openai_uri_is_rejected() {
        let uri: Uri = "https://example.com/v1/models".parse().unwrap();
        assert!(upstream_uri_for_cli_request(&uri).is_err());
    }

    #[test]
    fn connect_uri_uses_authority_with_port() {
        let uri: Uri = "api.openai.com:443".parse().unwrap();
        assert_eq!(connect_target_from_uri(&uri).unwrap(), "api.openai.com:443");
    }

    #[test]
    fn connect_uri_defaults_to_tls_port_when_missing() {
        let uri: Uri = "api.openai.com".parse().unwrap();
        assert_eq!(connect_target_from_uri(&uri).unwrap(), "api.openai.com:443");
    }

    #[test]
    fn proxy_runtime_is_sendable_enough_for_state_storage() {
        fn assert_send<T: Send>() {}
        assert_send::<ProxyRuntime>();
    }

    #[tokio::test]
    async fn reports_local_proxy_listener_health() {
        let runtime = start(ProxyStartOptions {
            port: 0,
            app_handle: None,
        })
        .await
        .unwrap();
        assert!(is_local_proxy_listening(runtime.addr.port()).await);
        runtime.stop().await;
    }

    #[test]
    fn stored_account_shape_remains_serializable_for_proxy_usage() {
        let mut account = StoredAccount::new_api_key("api".to_string(), "sk-test".to_string());
        account.id = Uuid::new_v4().to_string();
        account.created_at = Utc::now();
        assert!(bearer_token_for_account(&account).is_some());
    }

    #[test]
    fn lockfile_lives_under_authpilot_config_dir() {
        assert_eq!(
            lockfile_path_for_config_dir(Path::new("/tmp/authpilot-test")),
            PathBuf::from("/tmp/authpilot-test/.authpilot-proxy-active")
        );
    }
}
