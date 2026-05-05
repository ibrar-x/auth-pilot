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
}

export interface AccountSettings {
  switch_threshold: number;
}

export interface SwitchEvent {
  timestamp: string;
  from_account_id: string | null;
  to_account_id: string;
  reason: "auto_limit_reached" | "auto_depleted" | "manual";
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
