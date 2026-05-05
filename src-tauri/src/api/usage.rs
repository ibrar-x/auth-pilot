//! Usage API client for fetching rate limits and credits

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures::{stream, StreamExt};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, USER_AGENT},
    StatusCode,
};
use serde::Deserialize;
use std::collections::HashMap;

use crate::auth::{ensure_chatgpt_tokens_fresh, refresh_chatgpt_tokens};
use crate::types::{
    AuthData, CreditStatusDetails, RateLimitDetails, RateLimitStatusPayload, RateLimitWindow,
    StoredAccount, UsageInfo,
};

const CHATGPT_BACKEND_API: &str = "https://chatgpt.com/backend-api";
const CHATGPT_ACCOUNTS_CHECK_API: &str =
    "https://chatgpt.com/backend-api/accounts/check/v4-2023-04-27";
// const OPENAI_API: &str = "https://api.openai.com/v1";
const CODEX_USER_AGENT: &str = "codex-cli/1.0.0";

#[derive(Debug, Clone)]
pub struct ChatGptAccountMetadata {
    pub plan_type: Option<String>,
    pub subscription_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct AccountsCheckResponse {
    #[serde(default)]
    accounts: HashMap<String, AccountsCheckEntry>,
}

#[derive(Debug, Deserialize)]
struct AccountsCheckEntry {
    #[serde(default)]
    account: Option<AccountsCheckAccount>,
    #[serde(default)]
    entitlement: Option<AccountsCheckEntitlement>,
}

#[derive(Debug, Deserialize)]
struct AccountsCheckAccount {
    #[serde(default)]
    plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccountsCheckEntitlement {
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
}

pub async fn get_account_usage(account: &StoredAccount) -> Result<UsageInfo> {
    match &account.auth_data {
        AuthData::ApiKey { .. } => {
            Ok(UsageInfo {
                account_id: account.id.clone(),
                plan_type: Some("api_key".to_string()),
                primary_used_percent: None,
                primary_window_minutes: None,
                primary_resets_at: None,
                secondary_used_percent: None,
                secondary_window_minutes: None,
                secondary_resets_at: None,
                has_credits: None,
                unlimited_credits: None,
                credits_balance: None,
                error: Some("Usage info not available for API key accounts".to_string()),
            })
        }
        AuthData::ChatGPT { .. } => get_usage_with_chatgpt_auth(account).await,
    }
}

pub async fn fetch_chatgpt_account_metadata(
    account: &StoredAccount,
) -> Result<ChatGptAccountMetadata> {
    let (access_token, chatgpt_account_id) = extract_chatgpt_auth(account)?;
    let response =
        send_chatgpt_get_request(CHATGPT_ACCOUNTS_CHECK_API, access_token, chatgpt_account_id)
            .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Accounts check API error: {status} - {body}");
    }

    let payload: AccountsCheckResponse = response
        .json()
        .await
        .context("Failed to parse accounts check response")?;

    let selected_entry = chatgpt_account_id
        .and_then(|account_id| payload.accounts.get(account_id))
        .or_else(|| payload.accounts.get("default"))
        .or_else(|| payload.accounts.values().next())
        .context("Accounts check response did not include an account entry")?;

    Ok(ChatGptAccountMetadata {
        plan_type: selected_entry
            .account
            .as_ref()
            .and_then(|account| account.plan_type.clone()),
        subscription_expires_at: selected_entry
            .entitlement
            .as_ref()
            .and_then(|entitlement| entitlement.expires_at),
    })
}

async fn get_usage_with_chatgpt_auth(account: &StoredAccount) -> Result<UsageInfo> {
    let fresh_account = ensure_chatgpt_tokens_fresh(account).await?;
    let (access_token, chatgpt_account_id) = extract_chatgpt_auth(&fresh_account)?;

    let response = send_chatgpt_usage_request(access_token, chatgpt_account_id).await?;
    if response.status() == StatusCode::UNAUTHORIZED {
        let refreshed_account = refresh_chatgpt_tokens(&fresh_account).await?;
        let (retry_token, retry_account_id) = extract_chatgpt_auth(&refreshed_account)?;
        let retry_response = send_chatgpt_usage_request(retry_token, retry_account_id).await?;
        return parse_usage_response(
            &refreshed_account.id,
            &refreshed_account.name,
            retry_response,
        )
        .await;
    }

    parse_usage_response(&fresh_account.id, &fresh_account.name, response).await
}

async fn parse_usage_response(
    account_id: &str,
    _account_name: &str,
    response: reqwest::Response,
) -> Result<UsageInfo> {
    let status = response.status();

    if !status.is_success() {
        let _body = response.text().await.unwrap_or_default();
        return Ok(UsageInfo::error(
            account_id.to_string(),
            format!("API error: {status}"),
        ));
    }

    let body_text = response
        .text()
        .await
        .context("Failed to read response body")?;

    let payload: RateLimitStatusPayload =
        serde_json::from_str(&body_text).context("Failed to parse usage response")?;

    let usage = convert_payload_to_usage_info(account_id, payload);
    Ok(usage)
}

fn build_chatgpt_headers(
    access_token: &str,
    chatgpt_account_id: Option<&str>,
) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(CODEX_USER_AGENT));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}")).context("Invalid access token")?,
    );

    if let Some(acc_id) = chatgpt_account_id {
        if let Ok(header_name) = HeaderName::from_bytes(b"chatgpt-account-id") {
            if let Ok(header_value) = HeaderValue::from_str(acc_id) {
                headers.insert(header_name, header_value);
            }
        }
    }

    Ok(headers)
}

fn extract_chatgpt_auth(account: &StoredAccount) -> Result<(&str, Option<&str>)> {
    match &account.auth_data {
        AuthData::ChatGPT {
            access_token,
            account_id,
            ..
        } => Ok((access_token.as_str(), account_id.as_deref())),
        AuthData::ApiKey { .. } => anyhow::bail!("Account is not using ChatGPT OAuth"),
    }
}

async fn send_chatgpt_usage_request(
    access_token: &str,
    chatgpt_account_id: Option<&str>,
) -> Result<reqwest::Response> {
    send_chatgpt_get_request(
        &format!("{CHATGPT_BACKEND_API}/wham/usage"),
        access_token,
        chatgpt_account_id,
    )
    .await
}

async fn send_chatgpt_get_request(
    url: &str,
    access_token: &str,
    chatgpt_account_id: Option<&str>,
) -> Result<reqwest::Response> {
    let client = reqwest::Client::new();
    let headers = build_chatgpt_headers(access_token, chatgpt_account_id)?;

    client
        .get(url)
        .headers(headers)
        .send()
        .await
        .with_context(|| format!("Failed to send GET request to {url}"))
}

fn convert_payload_to_usage_info(account_id: &str, payload: RateLimitStatusPayload) -> UsageInfo {
    let (primary, secondary) = extract_rate_limits(payload.rate_limit);
    let credits = extract_credits(payload.credits);

    UsageInfo {
        account_id: account_id.to_string(),
        plan_type: Some(payload.plan_type),
        primary_used_percent: primary.as_ref().map(|w| w.used_percent),
        primary_window_minutes: primary
            .as_ref()
            .and_then(|w| w.limit_window_seconds)
            .map(|s| (i64::from(s) + 59) / 60),
        primary_resets_at: primary.as_ref().and_then(|w| w.reset_at),
        secondary_used_percent: secondary.as_ref().map(|w| w.used_percent),
        secondary_window_minutes: secondary
            .as_ref()
            .and_then(|w| w.limit_window_seconds)
            .map(|s| (i64::from(s) + 59) / 60),
        secondary_resets_at: secondary.as_ref().and_then(|w| w.reset_at),
        has_credits: credits.as_ref().map(|c| c.has_credits),
        unlimited_credits: credits.as_ref().map(|c| c.unlimited),
        credits_balance: credits.and_then(|c| c.balance),
        error: None,
    }
}

fn extract_rate_limits(
    rate_limit: Option<RateLimitDetails>,
) -> (Option<RateLimitWindow>, Option<RateLimitWindow>) {
    match rate_limit {
        Some(details) => (details.primary_window, details.secondary_window),
        None => (None, None),
    }
}

fn extract_credits(credits: Option<CreditStatusDetails>) -> Option<CreditStatusDetails> {
    credits
}

pub async fn refresh_all_usage(accounts: &[StoredAccount]) -> Vec<UsageInfo> {
    let concurrency = accounts.len().min(10).max(1);
    let results: Vec<UsageInfo> = stream::iter(accounts.iter().cloned())
        .map(|account| async move {
            match get_account_usage(&account).await {
                Ok(info) => info,
                Err(e) => {
                    tracing::error!("Error for {}: {}", account.name, e);
                    UsageInfo::error(account.id.clone(), e.to_string())
                }
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    results
}
