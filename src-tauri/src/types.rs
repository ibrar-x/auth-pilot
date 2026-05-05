use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountsStore {
    pub version: u32,
    pub accounts: Vec<StoredAccount>,
    pub active_account_id: Option<String>,
    #[serde(default)]
    pub masked_account_ids: Vec<String>,
}

impl Default for AccountsStore {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: Vec::new(),
            active_account_id: None,
            masked_account_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    #[serde(default)]
    pub subscription_expires_at: Option<DateTime<Utc>>,
    pub auth_mode: AuthMode,
    pub auth_data: AuthData,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl StoredAccount {
    pub fn new_api_key(name: String, api_key: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            email: None,
            plan_type: None,
            subscription_expires_at: None,
            auth_mode: AuthMode::ApiKey,
            auth_data: AuthData::ApiKey { key: api_key },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    pub fn new_chatgpt(
        name: String,
        email: Option<String>,
        plan_type: Option<String>,
        subscription_expires_at: Option<DateTime<Utc>>,
        id_token: String,
        access_token: String,
        refresh_token: String,
        account_id: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            email,
            plan_type,
            subscription_expires_at,
            auth_mode: AuthMode::ChatGPT,
            auth_data: AuthData::ChatGPT {
                id_token,
                access_token,
                refresh_token,
                account_id,
            },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    ApiKey,
    ChatGPT,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthData {
    ApiKey { key: String },
    ChatGPT {
        id_token: String,
        access_token: String,
        refresh_token: String,
        account_id: Option<String>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ChatGptIdTokenClaims {
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub account_id: Option<String>,
    pub subscription_expires_at: Option<DateTime<Utc>>,
}

pub fn parse_chatgpt_id_token_claims(id_token: &str) -> ChatGptIdTokenClaims {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return ChatGptIdTokenClaims::default();
    }

    let payload = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
        Ok(bytes) => bytes,
        Err(_) => return ChatGptIdTokenClaims::default(),
    };

    let json: serde_json::Value = match serde_json::from_slice(&payload) {
        Ok(value) => value,
        Err(_) => return ChatGptIdTokenClaims::default(),
    };

    let auth_claims = json.get("https://api.openai.com/auth");

    ChatGptIdTokenClaims {
        email: json.get("email").and_then(|v| v.as_str()).map(String::from),
        plan_type: auth_claims
            .and_then(|auth| auth.get("chatgpt_plan_type"))
            .and_then(|v| v.as_str())
            .map(String::from),
        account_id: auth_claims
            .and_then(|auth| auth.get("chatgpt_account_id"))
            .and_then(|v| v.as_str())
            .map(String::from),
        subscription_expires_at: auth_claims
            .and_then(|auth| auth.get("chatgpt_subscription_active_until"))
            .and_then(|v| v.as_str())
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthDotJson {
    #[serde(rename = "OPENAI_API_KEY", skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub subscription_expires_at: Option<DateTime<Utc>>,
    pub auth_mode: AuthMode,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl AccountInfo {
    pub fn from_stored(account: &StoredAccount, active_id: Option<&str>) -> Self {
        let fallback_subscription = match &account.auth_data {
            AuthData::ChatGPT { id_token, .. } => {
                parse_chatgpt_id_token_claims(id_token).subscription_expires_at
            }
            AuthData::ApiKey { .. } => None,
        };

        Self {
            id: account.id.clone(),
            name: account.name.clone(),
            email: account.email.clone(),
            plan_type: account.plan_type.clone(),
            subscription_expires_at: account
                .subscription_expires_at
                .clone()
                .or(fallback_subscription),
            auth_mode: account.auth_mode,
            is_active: active_id == Some(&account.id),
            created_at: account.created_at,
            last_used_at: account.last_used_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub account_id: String,
    pub plan_type: Option<String>,
    pub primary_used_percent: Option<f64>,
    pub primary_window_minutes: Option<i64>,
    pub primary_resets_at: Option<i64>,
    pub secondary_used_percent: Option<f64>,
    pub secondary_window_minutes: Option<i64>,
    pub secondary_resets_at: Option<i64>,
    pub has_credits: Option<bool>,
    pub unlimited_credits: Option<bool>,
    pub credits_balance: Option<String>,
    pub error: Option<String>,
}

impl UsageInfo {
    pub fn error(account_id: String, error: String) -> Self {
        Self {
            account_id,
            plan_type: None,
            primary_used_percent: None,
            primary_window_minutes: None,
            primary_resets_at: None,
            secondary_used_percent: None,
            secondary_window_minutes: None,
            secondary_resets_at: None,
            has_credits: None,
            unlimited_credits: None,
            credits_balance: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupSummary {
    pub total_accounts: usize,
    pub warmed_accounts: usize,
    pub failed_account_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportAccountsSummary {
    pub total_in_payload: usize,
    pub imported_count: usize,
    pub skipped_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthLoginInfo {
    pub auth_url: String,
    pub callback_port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitStatusPayload {
    pub plan_type: String,
    #[serde(default)]
    pub rate_limit: Option<RateLimitDetails>,
    #[serde(default)]
    pub credits: Option<CreditStatusDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitDetails {
    pub primary_window: Option<RateLimitWindow>,
    pub secondary_window: Option<RateLimitWindow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitWindow {
    pub used_percent: f64,
    pub limit_window_seconds: Option<i32>,
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreditStatusDetails {
    pub has_credits: bool,
    pub unlimited: bool,
    #[serde(default)]
    pub balance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub poll_interval_seconds: u64,
    pub notifications_enabled: bool,
    pub auto_switch_enabled: bool,
    pub global_cooldown_seconds: u64,
    pub last_auto_switch: Option<DateTime<Utc>>,
    pub account_settings: HashMap<String, AccountSettings>,
    pub theme: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            poll_interval_seconds: 60,
            notifications_enabled: true,
            auto_switch_enabled: true,
            global_cooldown_seconds: 300,
            last_auto_switch: None,
            account_settings: HashMap::new(),
            theme: String::from("system"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSettings {
    pub switch_threshold: f64,
}

impl Default for AccountSettings {
    fn default() -> Self {
        Self {
            switch_threshold: 95.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchEvent {
    pub timestamp: DateTime<Utc>,
    pub from_account_id: Option<String>,
    pub to_account_id: String,
    pub reason: SwitchReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchReason {
    AutoLimitReached,
    AutoDepleted,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchLog {
    pub events: Vec<SwitchEvent>,
    #[serde(default = "default_max_events")]
    pub max_events: usize,
}

fn default_max_events() -> usize {
    20
}

impl Default for SwitchLog {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            max_events: 20,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MonitorState {
    pub settings: AppSettings,
    pub latest_usages: Vec<UsageInfo>,
    pub is_monitor_running: bool,
    pub cached_accounts: Option<AccountsStore>,
}

#[cfg(test)]
mod tests {
    use super::parse_chatgpt_id_token_claims;
    use base64::Engine;

    #[test]
    fn parses_subscription_expiry_from_realistic_id_token_claims() {
        let payload = r#"{"email":"user@example.com","https://api.openai.com/auth":{"chatgpt_plan_type":"plus","chatgpt_account_id":"acc_123","chatgpt_subscription_active_until":"2026-04-23T05:03:38+00:00"}}"#;
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        let token = format!("header.{encoded}.signature");

        let claims = parse_chatgpt_id_token_claims(&token);

        assert_eq!(claims.email.as_deref(), Some("user@example.com"));
        assert_eq!(claims.plan_type.as_deref(), Some("plus"));
        assert_eq!(claims.account_id.as_deref(), Some("acc_123"));
        assert_eq!(
            claims.subscription_expires_at.map(|v| v.to_rfc3339()),
            Some("2026-04-23T05:03:38+00:00".to_string())
        );
    }
}
