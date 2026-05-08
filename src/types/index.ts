// Types matching the Rust backend

export type AuthMode = "api_key" | "chat_g_p_t";

export interface AccountInfo {
  id: string;
  name: string;
  email: string | null;
  plan_type: string | null;
  subscription_expires_at: string | null;
  auth_mode: AuthMode;
  is_active: boolean;
  created_at: string;
  last_used_at: string | null;
}

export interface UsageInfo {
  account_id: string;
  plan_type: string | null;
  primary_used_percent: number | null;
  primary_window_minutes: number | null;
  primary_resets_at: number | null;
  secondary_used_percent: number | null;
  secondary_window_minutes: number | null;
  secondary_resets_at: number | null;
  has_credits: boolean | null;
  unlimited_credits: boolean | null;
  credits_balance: string | null;
  error: string | null;
}

export interface AccountWithUsage extends AccountInfo {
  usage?: UsageInfo;
  usageLoading?: boolean;
}

export interface OAuthLoginInfo {
  auth_url: string;
  callback_port: number;
}

export interface AppSettings {
  poll_interval_seconds: number;
  notifications_enabled: boolean;
  auto_switch_enabled: boolean;
  global_cooldown_seconds: number;
  last_auto_switch: string | null;
  account_settings: Record<string, AccountSettings>;
  theme: "light" | "dark" | "system";
  usage_display_mode?: UsageDisplayMode;
  start_at_login?: boolean;
  show_in_dock?: boolean;
  privacy_mode_enabled?: boolean;
  privacy_mask_style?: PrivacyMaskStyle;
  privacy_replacement_text?: string;
  dashboard_global_shortcut?: string;
  auto_resume_after_restart?: boolean;
}

export type UsageDisplayMode = "remaining" | "used";
export type PrivacyMaskStyle = "blur" | "replace";

export interface AccountSettings {
  switch_threshold: number;
}

export interface SwitchEvent {
  timestamp: string;
  from_account_id: string | null;
  to_account_id: string;
  reason: "auto_limit_reached" | "auto_depleted" | "manual";
  codex_was_running?: boolean;
  codex_stopped_at?: string | null;
  codex_restarted_at?: string | null;
  recovery_session_id?: string | null;
  auto_resume_attempted?: boolean;
  auto_resume_started?: boolean;
}

export interface WarmupSummary {
  total_accounts: number;
  attempted: number;
  succeeded: number;
  failed: number;
  failures: { account_id: string; error: string }[];
}

export interface ImportAccountsSummary {
  total_in_payload: number;
  imported_count: number;
  skipped_count: number;
}

export type SessionStatus =
  | "running"
  | "completed"
  | "interrupted"
  | "ignored"
  | "background_resumed";

export interface CodexSession {
  id: string;
  thread_id: string | null;
  session_id: string | null;
  workspace_path: string;
  account_id: string | null;
  process_id: number | null;
  started_at: string;
  last_seen_at: string;
  ended_at: string | null;
  status: SessionStatus;
  recovery_attempts: number;
  last_recovery_at: string | null;
  last_recovery_prompt: string | null;
}

export type ReopenOutcome =
  | { type: "deeplink_opened" }
  | { type: "workspace_opened" }
  | { type: "manual_required"; data: { workspace_path: string | null } };

export type BackgroundResumeOutcome =
  | { type: "started"; data: { log_path: string } }
  | { type: "cooldown_active"; data: { seconds_remaining: number } }
  | { type: "max_attempts_reached" };
