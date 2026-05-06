use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::crypto::{decrypt, encrypt, get_machine_id, EncryptedBlob};
use crate::types::{
    parse_chatgpt_id_token_claims, AccountsStore, AuthData, AuthDotJson, StoredAccount,
};

pub fn get_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home.join(".authpilot"))
}

pub fn get_legacy_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home.join(".codex-switcher"))
}

/// Migrate accounts from old ~/.codex-switcher/ to new ~/.authpilot/
pub fn migrate_legacy_accounts() -> Result<bool> {
    let legacy_dir = get_legacy_config_dir()?;
    let legacy_file = legacy_dir.join("accounts.json");

    if !legacy_file.exists() {
        return Ok(false); // No legacy data to migrate
    }

    let new_dir = get_config_dir()?;
    let new_file = new_dir.join("accounts.json");

    // Only migrate if new file doesn't exist yet
    if new_file.exists() {
        return Ok(false);
    }

    // Read legacy file (may be plain JSON or encrypted)
    let content = fs::read_to_string(&legacy_file).with_context(|| {
        format!(
            "Failed to read legacy accounts file: {}",
            legacy_file.display()
        )
    })?;

    // Try to parse as encrypted blob first
    let store: AccountsStore = if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        let machine_id =
            get_machine_id().context("Failed to get machine ID for legacy migration")?;
        let plaintext = decrypt(&blob, &machine_id)
            .with_context(|| "Failed to decrypt legacy accounts file")?;
        serde_json::from_str(&plaintext)
            .context("Failed to parse decrypted legacy accounts file")?
    } else {
        // Plain JSON
        serde_json::from_str(&content).with_context(|| {
            format!(
                "Failed to parse legacy accounts file: {}",
                legacy_file.display()
            )
        })?
    };

    // Create new config directory
    fs::create_dir_all(&new_dir).with_context(|| {
        format!(
            "Failed to create new config directory: {}",
            new_dir.display()
        )
    })?;

    // Save to new location (will be encrypted automatically)
    save_accounts(&store)?;

    // Create auth.json snapshots for all migrated accounts
    for account in &store.accounts {
        if let Err(e) = crate::session::snapshot_account_from_data(account) {
            tracing::warn!(
                "Failed to create snapshot for migrated account {}: {}",
                account.id,
                e
            );
        }
    }

    tracing::info!(
        "Migrated {} accounts from legacy ~/.codex-switcher/ to ~/.authpilot/",
        store.accounts.len()
    );
    Ok(true)
}

pub fn get_accounts_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("accounts.json"))
}

pub fn load_accounts() -> Result<AccountsStore> {
    let path = get_accounts_file()?;

    if !path.exists() {
        return Ok(AccountsStore::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read accounts file: {}", path.display()))?;

    // Try encrypted format first
    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        let persisted_id = get_machine_id().context("Failed to get machine ID")?;

        // 1. Try the stable persisted machine ID
        if let Ok(plaintext) = decrypt(&blob, &persisted_id) {
            let store: AccountsStore = serde_json::from_str(&plaintext)
                .context("Failed to parse decrypted accounts file")?;
            return Ok(store);
        }

        // 2. Fallback: try the raw computed ID (for data encrypted before persistence was added)
        let computed_id =
            crate::crypto::compute_machine_id().context("Failed to compute machine ID fallback")?;
        if let Ok(plaintext) = decrypt(&blob, &computed_id) {
            let store: AccountsStore = serde_json::from_str(&plaintext)
                .context("Failed to parse decrypted accounts file")?;
            tracing::info!(
                "Recovered accounts using computed machine ID; re-encrypting with persisted ID"
            );
            let _ = save_accounts(&store); // Re-save with stable ID
            return Ok(store);
        }

        anyhow::bail!("Failed to decrypt accounts file (wrong machine or corrupted data)")
    }

    // Fall back to plain JSON
    let store: AccountsStore = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse accounts file: {}", path.display()))?;

    Ok(store)
}

pub fn load_accounts_with_current_active() -> Result<AccountsStore> {
    let mut store = load_accounts()?;
    sync_active_account_from_current_auth(&mut store)?;
    Ok(store)
}

pub fn sync_active_account_from_current_auth(store: &mut AccountsStore) -> Result<Option<String>> {
    let auth = match crate::auth::switcher::read_current_auth()? {
        Some(auth) => auth,
        None => return Ok(store.active_account_id.clone()),
    };

    let previous_active_id = store.active_account_id.clone();
    let resolved_id = sync_active_account_from_auth(store, &auth);

    if store.active_account_id != previous_active_id {
        save_accounts(store)?;
    }

    Ok(resolved_id)
}

fn sync_active_account_from_auth(store: &mut AccountsStore, auth: &AuthDotJson) -> Option<String> {
    let matches: Vec<(String, u8)> = store
        .accounts
        .iter()
        .filter_map(|account| {
            let score = account_auth_match_score(account, auth);
            (score > 0).then(|| (account.id.clone(), score))
        })
        .collect();

    let highest_score = matches.iter().map(|(_, score)| *score).max();
    let Some(highest_score) = highest_score else {
        return store.active_account_id.clone();
    };

    let best_ids: Vec<&String> = matches
        .iter()
        .filter(|(_, score)| *score == highest_score)
        .map(|(id, _)| id)
        .collect();

    if let Some(active_id) = store.active_account_id.as_deref() {
        if best_ids.iter().any(|id| id.as_str() == active_id) {
            return Some(active_id.to_string());
        }
    }

    match best_ids.as_slice() {
        [resolved_id] => {
            if store.active_account_id.as_deref() != Some(resolved_id.as_str()) {
                store.active_account_id = Some((*resolved_id).clone());
            }
            Some((*resolved_id).clone())
        }
        _ => {
            tracing::warn!(
                "Current auth matches multiple stored accounts equally; selecting first best match"
            );
            let resolved_id = (*best_ids[0]).clone();
            store.active_account_id = Some(resolved_id.clone());
            Some(resolved_id)
        }
    }
}

fn account_auth_match_score(account: &StoredAccount, auth: &AuthDotJson) -> u8 {
    match (&account.auth_data, auth) {
        (
            AuthData::ApiKey { key },
            AuthDotJson {
                openai_api_key: Some(current_key),
                ..
            },
        ) if key == current_key => 100,
        (AuthData::ApiKey { .. }, _) => 0,
        (
            AuthData::ChatGPT {
                id_token,
                access_token,
                refresh_token,
                account_id,
            },
            AuthDotJson {
                tokens: Some(current_tokens),
                ..
            },
        ) => {
            if access_token == &current_tokens.access_token
                || refresh_token == &current_tokens.refresh_token
                || id_token == &current_tokens.id_token
            {
                return 100;
            }

            let stored_claims = parse_chatgpt_id_token_claims(id_token);
            let current_claims = parse_chatgpt_id_token_claims(&current_tokens.id_token);

            let stored_account_id = account_id
                .as_deref()
                .or(stored_claims.account_id.as_deref());
            let current_account_id = current_tokens
                .account_id
                .as_deref()
                .or(current_claims.account_id.as_deref());

            if stored_account_id.is_some() && stored_account_id == current_account_id {
                return 80;
            }

            let stored_email = account.email.as_deref().or(stored_claims.email.as_deref());
            let current_email = current_claims.email.as_deref();
            if stored_account_id.is_none()
                && current_account_id.is_none()
                && stored_email.is_some()
                && stored_email == current_email
            {
                10
            } else {
                0
            }
        }
        _ => 0,
    }
}

pub fn save_accounts(store: &AccountsStore) -> Result<()> {
    let path = get_accounts_file()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let plaintext =
        serde_json::to_string_pretty(store).context("Failed to serialize accounts store")?;

    let machine_id = get_machine_id().context("Failed to get machine ID")?;
    let blob = encrypt(&plaintext, &machine_id).context("Failed to encrypt accounts")?;
    let content = serde_json::to_string(&blob).context("Failed to serialize encrypted blob")?;

    fs::write(&path, content)
        .with_context(|| format!("Failed to write accounts file: {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

pub fn add_account(account: StoredAccount) -> Result<StoredAccount> {
    let mut store = load_accounts()?;

    if store.accounts.iter().any(|a| a.name == account.name) {
        anyhow::bail!("An account with name '{}' already exists", account.name);
    }

    let account_clone = account.clone();
    store.accounts.push(account);

    if store.accounts.len() == 1 {
        store.active_account_id = Some(account_clone.id.clone());
    }

    save_accounts(&store)?;
    Ok(account_clone)
}

pub fn remove_account(account_id: &str) -> Result<()> {
    let mut store = load_accounts()?;

    let initial_len = store.accounts.len();
    store.accounts.retain(|a| a.id != account_id);

    if store.accounts.len() == initial_len {
        anyhow::bail!("Account not found: {account_id}");
    }

    if store.active_account_id.as_deref() == Some(account_id) {
        store.active_account_id = store.accounts.first().map(|a| a.id.clone());
    }

    save_accounts(&store)?;
    Ok(())
}

pub fn set_active_account(account_id: &str) -> Result<()> {
    let mut store = load_accounts()?;

    if !store.accounts.iter().any(|a| a.id == account_id) {
        anyhow::bail!("Account not found: {account_id}");
    }

    store.active_account_id = Some(account_id.to_string());
    save_accounts(&store)?;
    Ok(())
}

pub fn get_account(account_id: &str) -> Result<Option<StoredAccount>> {
    let store = load_accounts()?;
    Ok(store.accounts.into_iter().find(|a| a.id == account_id))
}

pub fn get_active_account() -> Result<Option<StoredAccount>> {
    let store = load_accounts_with_current_active()?;
    let active_id = match &store.active_account_id {
        Some(id) => id,
        None => return Ok(None),
    };
    Ok(store.accounts.into_iter().find(|a| a.id == *active_id))
}

pub fn touch_account(account_id: &str) -> Result<()> {
    let mut store = load_accounts()?;

    if let Some(account) = store.accounts.iter_mut().find(|a| a.id == account_id) {
        account.last_used_at = Some(chrono::Utc::now());
        save_accounts(&store)?;
    }

    Ok(())
}

pub fn update_account_metadata(
    account_id: &str,
    name: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
    subscription_expires_at: Option<Option<DateTime<Utc>>>,
) -> Result<StoredAccount> {
    let mut store = load_accounts()?;

    if let Some(ref new_name) = name {
        if store
            .accounts
            .iter()
            .any(|a| a.id != account_id && a.name == *new_name)
        {
            anyhow::bail!("An account with name '{new_name}' already exists");
        }
    }

    let account = store
        .accounts
        .iter_mut()
        .find(|a| a.id == account_id)
        .context("Account not found")?;

    if let Some(new_name) = name {
        account.name = new_name;
    }

    if email.is_some() {
        account.email = email;
    }

    if plan_type.is_some() {
        account.plan_type = plan_type;
    }

    if let Some(subscription_expires_at) = subscription_expires_at {
        account.subscription_expires_at = subscription_expires_at;
    }

    let updated = account.clone();
    save_accounts(&store)?;
    Ok(updated)
}

#[allow(clippy::too_many_arguments)]
pub fn update_account_chatgpt_tokens(
    account_id: &str,
    id_token: String,
    access_token: String,
    refresh_token: String,
    chatgpt_account_id: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
    subscription_expires_at: Option<DateTime<Utc>>,
) -> Result<StoredAccount> {
    let mut store = load_accounts()?;

    let account = store
        .accounts
        .iter_mut()
        .find(|a| a.id == account_id)
        .context("Account not found")?;

    match &mut account.auth_data {
        AuthData::ChatGPT {
            id_token: stored_id_token,
            access_token: stored_access_token,
            refresh_token: stored_refresh_token,
            account_id: stored_account_id,
        } => {
            *stored_id_token = id_token;
            *stored_access_token = access_token;
            *stored_refresh_token = refresh_token;
            if let Some(new_account_id) = chatgpt_account_id {
                *stored_account_id = Some(new_account_id);
            }
        }
        AuthData::ApiKey { .. } => {
            anyhow::bail!("Cannot update OAuth tokens for an API key account");
        }
    }

    if let Some(new_email) = email {
        account.email = Some(new_email);
    }

    if let Some(new_plan_type) = plan_type {
        account.plan_type = Some(new_plan_type);
    }

    if let Some(subscription_expires_at) = subscription_expires_at {
        account.subscription_expires_at = Some(subscription_expires_at);
    }

    let updated = account.clone();
    save_accounts(&store)?;
    Ok(updated)
}

pub fn get_masked_account_ids() -> Result<Vec<String>> {
    let store = load_accounts()?;
    Ok(store.masked_account_ids.clone())
}

pub fn set_masked_account_ids(ids: Vec<String>) -> Result<()> {
    let mut store = load_accounts()?;
    store.masked_account_ids = ids;
    save_accounts(&store)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TokenData;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    fn id_token_with_email(email: &str) -> String {
        let payload = serde_json::json!({ "email": email });
        format!(
            "header.{}.signature",
            URL_SAFE_NO_PAD.encode(payload.to_string())
        )
    }

    #[test]
    fn matches_api_key_auth_to_stored_account() {
        let account = StoredAccount::new_api_key("api".to_string(), "sk-test".to_string());
        let auth = AuthDotJson {
            openai_api_key: Some("sk-test".to_string()),
            tokens: None,
            last_refresh: None,
        };

        assert!(account_auth_match_score(&account, &auth) > 0);
    }

    #[test]
    fn matches_chatgpt_auth_by_account_id_after_tokens_change() {
        let account = StoredAccount::new_chatgpt(
            "chatgpt".to_string(),
            Some("user@example.com".to_string()),
            None,
            None,
            "old-id-token".to_string(),
            "old-access-token".to_string(),
            "old-refresh-token".to_string(),
            Some("acc_123".to_string()),
        );
        let auth = AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: "new-id-token".to_string(),
                access_token: "new-access-token".to_string(),
                refresh_token: "new-refresh-token".to_string(),
                account_id: Some("acc_123".to_string()),
            }),
            last_refresh: None,
        };

        assert!(account_auth_match_score(&account, &auth) > 0);
    }

    #[test]
    fn does_not_match_different_chatgpt_account_id() {
        let account = StoredAccount::new_chatgpt(
            "chatgpt".to_string(),
            Some("user@example.com".to_string()),
            None,
            None,
            "old-id-token".to_string(),
            "old-access-token".to_string(),
            "old-refresh-token".to_string(),
            Some("acc_123".to_string()),
        );
        let auth = AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: "new-id-token".to_string(),
                access_token: "new-access-token".to_string(),
                refresh_token: "new-refresh-token".to_string(),
                account_id: Some("acc_456".to_string()),
            }),
            last_refresh: None,
        };

        assert_eq!(account_auth_match_score(&account, &auth), 0);
    }

    #[test]
    fn keeps_current_active_account_when_multiple_accounts_match_same_auth() {
        let first = StoredAccount::new_chatgpt(
            "first".to_string(),
            Some("user@example.com".to_string()),
            None,
            None,
            id_token_with_email("user@example.com"),
            "old-access-token".to_string(),
            "old-refresh-token".to_string(),
            None,
        );
        let second = StoredAccount::new_chatgpt(
            "second".to_string(),
            Some("user@example.com".to_string()),
            None,
            None,
            "new-id-token".to_string(),
            "new-access-token".to_string(),
            "new-refresh-token".to_string(),
            None,
        );
        let auth = AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: "new-id-token".to_string(),
                access_token: "new-access-token".to_string(),
                refresh_token: "new-refresh-token".to_string(),
                account_id: None,
            }),
            last_refresh: None,
        };

        let mut store = AccountsStore {
            version: 1,
            active_account_id: Some(second.id.clone()),
            accounts: vec![first, second.clone()],
            masked_account_ids: Vec::new(),
        };

        sync_active_account_from_auth(&mut store, &auth);

        assert_eq!(store.active_account_id.as_deref(), Some(second.id.as_str()));
    }

    #[test]
    fn prefers_exact_token_match_over_active_email_match() {
        let active_email_match = StoredAccount::new_chatgpt(
            "active-email-match".to_string(),
            Some("user@example.com".to_string()),
            None,
            None,
            "old-id-token".to_string(),
            "old-access-token".to_string(),
            "old-refresh-token".to_string(),
            None,
        );
        let exact_token_match = StoredAccount::new_chatgpt(
            "exact-token-match".to_string(),
            Some("user@example.com".to_string()),
            None,
            None,
            id_token_with_email("user@example.com"),
            "current-access-token".to_string(),
            "current-refresh-token".to_string(),
            None,
        );
        let auth = AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: id_token_with_email("user@example.com"),
                access_token: "current-access-token".to_string(),
                refresh_token: "current-refresh-token".to_string(),
                account_id: None,
            }),
            last_refresh: None,
        };

        let mut store = AccountsStore {
            version: 1,
            active_account_id: Some(active_email_match.id.clone()),
            accounts: vec![active_email_match, exact_token_match.clone()],
            masked_account_ids: Vec::new(),
        };

        sync_active_account_from_auth(&mut store, &auth);

        assert_eq!(
            store.active_account_id.as_deref(),
            Some(exact_token_match.id.as_str())
        );
    }
}
