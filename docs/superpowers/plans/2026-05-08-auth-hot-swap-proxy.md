# Auth Hot-Swap Proxy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to execute this plan.

**Goal:** Add AuthPilot-managed local proxy foundations so Codex CLI requests can use the currently active AuthPilot account without rewriting `auth.json` or restarting Codex, then stage the privileged macOS desktop interception work behind explicit consent and cleanup controls.

**Architecture:** Introduce a localhost-only proxy module with pure auth-injection helpers, a process-local runtime handle, and a narrowly scoped HTTP forwarding path for CLI use. The proxy reads active credentials from the existing encrypted account store on each request, strips incoming OpenAI `Authorization`, injects the active account token only for exact `api.openai.com`, and leaves non-OpenAI traffic untouched. Future phases add CLI wrapper automation, CA/TLS MITM, system proxy management, launchd, and network watchers as separate consent-gated layers.

**Tech Stack:** Rust, Tauri 2, Tokio, reqwest, existing encrypted account storage, existing monitor/switch state, focused Rust unit tests.

---

## Security Invariants

- Bind only to `127.0.0.1`; never expose the proxy on LAN interfaces.
- Never log account tokens, authorization header values, or request bodies.
- Only inject credentials for exact `api.openai.com`.
- Strip client-provided `Authorization` only when injecting AuthPilot credentials for OpenAI.
- Preserve non-OpenAI requests without credential mutation.
- Do not install CA certificates, modify macOS system proxy settings, or edit shell startup files until the app has explicit user-facing consent and cleanup paths.
- All privileged system changes must have best-effort restore on shutdown and stale-lock recovery on next launch.

---

## Phase 1: CLI HTTP Proxy Core

**Deliverable:** A localhost proxy foundation that can identify OpenAI traffic, produce the correct active account bearer token, and forward CLI-style requests to `https://api.openai.com`.

### Task 1.1: Add Proxy Module Skeleton

- Add `src-tauri/src/proxy/mod.rs`.
- Export it from `src-tauri/src/lib.rs`.
- Define:
  - `DEFAULT_PROXY_PORT: u16 = 18080`
  - `ProxyRuntime`
  - `ProxyStartOptions`
  - `ProxyError` handling through `anyhow::Result`
- Keep lifecycle start functions inert unless explicitly called by later integration code.

### Task 1.2: Add Auth Header Helpers First

- Implement `should_inject_auth_host(host: &str) -> bool`.
- Implement `bearer_token_for_account(account: &StoredAccount) -> Option<String>`.
- Implement `active_bearer_token() -> Result<Option<String>>` using `auth::storage::get_active_account()`.
- Add tests:
  - exact `api.openai.com` returns true.
  - subdomains and lookalikes return false.
  - API key account maps to bearer token.
  - ChatGPT account maps to access token.
  - empty token strings return none.

### Task 1.3: Add Request Header Mutation Helpers

- Implement a helper that receives host and mutable headers.
- For OpenAI host with an active token:
  - remove existing `authorization`.
  - insert `Authorization: Bearer <active-token>`.
- For non-OpenAI host:
  - leave headers untouched.
- Add tests for stripping, injection, and non-OpenAI preservation.

### Task 1.4: Add CLI Forwarding Path

- Add a Tokio TCP listener bound to `127.0.0.1:<port>`.
- Accept CLI-style HTTP requests where the incoming path maps to `https://api.openai.com{path}`.
- Forward method, path, query, headers, and body with `reqwest`.
- Preserve response status and key content headers.
- Stream when possible; if the chosen first implementation buffers, document the gap and keep CONNECT/TLS out of scope until Phase 5.

### Task 1.5: Verify

- Run `cargo test`.
- Run `cargo check`.

---

## Phase 2: Switch Executor Integration

**Deliverable:** Account switches rotate proxy credentials without restarting Codex when proxy mode is enabled.

### Task 2.1: Add Settings Fields

- Add to `AppSettings`:
  - `proxy_mode_enabled: bool`
  - `proxy_port: u16`
  - `proxy_cli_wrapper_enabled: bool`
- Defaults:
  - `proxy_mode_enabled: false` until consent UI exists.
  - `proxy_port: 18080`.
  - `proxy_cli_wrapper_enabled: false`.
- Mirror TypeScript settings types and settings UI later.

### Task 2.2: Update Switch Path

- In `switch_executor`, if `proxy_mode_enabled` is true and proxy runtime is healthy:
  - update active account in storage.
  - do not kill/relaunch Codex.
  - emit a switch event and notification.
- Otherwise keep existing kill/relaunch mode.

### Task 2.3: Verify

- Add tests around branch selection where possible.
- Run `cargo test`.

---

## Phase 3: CLI Wrapper Automation

**Deliverable:** User can install/remove a managed `codex` wrapper that points CLI traffic at AuthPilot when the proxy is listening.

### Task 3.1: Implement Wrapper Generator

- Add `src-tauri/src/cli_wrapper.rs`.
- Generate wrapper script with clear AuthPilot marker.
- Prefer managed wrapper path only when safe backup is possible.
- Fallback to no-op warning until a UI consent path exists; do not silently edit shell RC files in this phase.

### Task 3.2: Add Commands

- Add Tauri commands:
  - `install_cli_wrapper`
  - `remove_cli_wrapper`
  - `get_cli_wrapper_status`
- Surface later in Settings.

### Task 3.3: Verify

- Unit test wrapper marker detection and generated content.
- Run `cargo test`.

---

## Phase 4: Lifecycle And Crash Cleanup

**Deliverable:** Proxy runtime and any managed wrapper state can be stopped or restored cleanly.

### Task 4.1: Runtime Handle

- Store a proxy runtime handle in Tauri state.
- Start proxy during setup only when `proxy_mode_enabled` is true.
- Stop proxy on app exit.

### Task 4.2: Lockfile

- Add `.authpilot-proxy-active` lockfile under AuthPilot config dir.
- Clear on shutdown.
- On startup, detect stale lockfile and repair only AuthPilot-owned state.

### Task 4.3: Verify

- Unit test lockfile path and stale detection helpers.
- Run `cargo test`.

---

## Phase 5: Desktop HTTPS Interception Preparation

**Deliverable:** Certificate generation and consent UI plumbing, without enabling system proxy automatically.

### Task 5.1: CA Generation

- Add CA generation/loading under app data/config dir.
- Store private key mode `0600`.
- Never install into Keychain without explicit frontend confirmation.

### Task 5.2: Consent Events

- Emit `ca-install-required`.
- Add commands to confirm or decline.
- Persist only explicit approval.

### Task 5.3: Verify

- Unit test cert file generation behavior where practical.
- Run `cargo test`.

---

## Phase 6: macOS System Proxy And CONNECT

**Deliverable:** Desktop Codex traffic can route through AuthPilot after user consent.

### Task 6.1: System Proxy Manager

- Enumerate active network services.
- Save exact services modified.
- Enable/disable HTTPS proxy only for saved services.
- Add stale-lock recovery.

### Task 6.2: CONNECT/TLS Handler

- Implement CONNECT tunnel handling.
- Generate per-host certificates from the local CA.
- Only inject auth for exact `api.openai.com`.
- Pass all other traffic through unmodified.

### Task 6.3: Verify

- Add unit tests for service parsing and host injection.
- Manually test on a disposable network configuration before shipping.

---

## Phase 7: launchd And Network Watcher

**Deliverable:** AuthPilot can restore proxy mode on login and after network interface changes.

### Task 7.1: launchd Agent

- Generate `~/Library/LaunchAgents/com.authpilot.agent.plist`.
- Respect `start_at_login`.
- Support `--background` mode.

### Task 7.2: Network Watcher

- Reapply proxy to newly active services.
- Debounce polling.
- Do not overwrite unrelated user proxy settings.

### Task 7.3: Verify

- Test plist rendering.
- Manual login/reboot test before release.

---

## Phase 8: 401 And Exhaustion Hardening

**Deliverable:** Proxy detects expired tokens and integrates with existing auto-switch behavior.

### Task 8.1: 401 Detection

- Detect OpenAI `401`.
- Emit `token-expired` without logging token data.
- Trigger force-switch path.

### Task 8.2: No Target Handling

- Emit `no-switch-target` when every account is exhausted.
- Preserve upstream response.

### Task 8.3: Verify

- Add response-status tests with mocked upstream where practical.
- Run `cargo test`.

