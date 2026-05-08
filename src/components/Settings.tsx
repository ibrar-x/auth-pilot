import { type KeyboardEvent, useEffect, useState } from "react";
import type {
  AppSettings,
  AccountWithUsage,
  CaStatus,
  CliWrapperStatus,
  SystemProxyStatus,
} from "../types";
import { invokeBackend } from "../lib/platform";
import { applyTheme } from "../lib/theme";

interface SettingsProps {
  settings: AppSettings;
  onSave: (settings: AppSettings) => Promise<void>;
  onClose: () => void;
  accounts: AccountWithUsage[];
}

const THEMES = [
  { value: "light", label: "Light", icon: "☀️" },
  { value: "dark", label: "Dark", icon: "🌙" },
  { value: "system", label: "System", icon: "⚙️" },
] as const;

const USAGE_DISPLAY_MODES = [
  {
    value: "remaining",
    label: "Remaining",
    description: "Bars shrink as quota is used.",
  },
  {
    value: "used",
    label: "Used",
    description: "Bars fill as quota is used.",
  },
] as const;

const PRIVACY_MASK_STYLES = [
  {
    value: "blur",
    label: "Blur",
    description: "Keep layout intact and blur account details.",
  },
  {
    value: "replace",
    label: "Replace",
    description: "Show one safe word instead of names and emails.",
  },
] as const;

const DEFAULT_DASHBOARD_SHORTCUT = "CommandOrControl+Shift+A";

function getDefaultSettings(): AppSettings {
  return {
    poll_interval_seconds: 60,
    notifications_enabled: true,
    auto_switch_enabled: true,
    proxy_mode_enabled: false,
    proxy_port: 18080,
    proxy_cli_wrapper_enabled: false,
    proxy_ca_trusted: false,
    global_cooldown_seconds: 300,
    last_auto_switch: null,
    account_settings: {},
    theme: "system",
    usage_display_mode: "remaining",
    start_at_login: false,
    show_in_dock: false,
    privacy_mode_enabled: false,
    privacy_mask_style: "blur",
    privacy_replacement_text: "Hidden",
    dashboard_global_shortcut: DEFAULT_DASHBOARD_SHORTCUT,
  };
}

function keyToShortcutPart(key: string): string | null {
  if (key === " ") return "Space";
  if (key.length === 1) return key.toUpperCase();

  const normalizedKeys: Record<string, string> = {
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    ArrowUp: "Up",
    Backspace: "Backspace",
    Delete: "Delete",
    End: "End",
    Enter: "Enter",
    Escape: "Esc",
    Home: "Home",
    Insert: "Insert",
    PageDown: "PageDown",
    PageUp: "PageUp",
    Tab: "Tab",
  };

  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(key)) return key;
  return normalizedKeys[key] ?? null;
}

function shortcutFromKeyboardEvent(event: KeyboardEvent<HTMLElement>): string | null {
  const key = keyToShortcutPart(event.key);
  if (!key) return null;

  const modifiers: string[] = [];
  if (event.metaKey || event.ctrlKey) modifiers.push("CommandOrControl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");

  if (modifiers.length === 0) return null;
  return [...modifiers, key].join("+");
}

function formatShortcutLabel(shortcut: string | undefined): string {
  if (!shortcut?.trim()) return "Disabled";

  return shortcut
    .trim()
    .split("+")
    .map((part) => {
      if (part === "CommandOrControl") return "Cmd/Ctrl";
      if (part === "Alt") return "Option";
      if (part === "Shift") return "Shift";
      return part;
    })
    .join(" + ");
}

export function Settings({ settings, onSave, onClose, accounts }: SettingsProps) {
  const [formSettings, setFormSettings] = useState<AppSettings>({ ...settings });
  const [saving, setSaving] = useState(false);
  const [saveSuccess, setSaveSuccess] = useState(false);
  const [exportSuccess, setExportSuccess] = useState(false);
  const [isRecordingShortcut, setIsRecordingShortcut] = useState(false);
  const [wrapperStatus, setWrapperStatus] = useState<CliWrapperStatus | null>(null);
  const [wrapperBusy, setWrapperBusy] = useState(false);
  const [wrapperMessage, setWrapperMessage] = useState<string | null>(null);
  const [caStatus, setCaStatus] = useState<CaStatus | null>(null);
  const [caBusy, setCaBusy] = useState(false);
  const [caMessage, setCaMessage] = useState<string | null>(null);
  const [systemProxyStatus, setSystemProxyStatus] = useState<SystemProxyStatus | null>(null);
  const [systemProxyBusy, setSystemProxyBusy] = useState(false);
  const [systemProxyMessage, setSystemProxyMessage] = useState<string | null>(null);

  useEffect(() => {
    invokeBackend<CliWrapperStatus>("get_cli_wrapper_status")
      .then(setWrapperStatus)
      .catch(() => setWrapperStatus(null));
    invokeBackend<CaStatus>("get_proxy_ca_status")
      .then(setCaStatus)
      .catch(() => setCaStatus(null));
    invokeBackend<SystemProxyStatus>("get_system_proxy_status")
      .then(setSystemProxyStatus)
      .catch(() => setSystemProxyStatus(null));
  }, []);

  const handleSave = async () => {
    try {
      setSaving(true);
      await onSave(formSettings);
      applyTheme(formSettings.theme);
      setSaveSuccess(true);
      setTimeout(() => setSaveSuccess(false), 2000);
    } catch (err) {
      console.error("Failed to save settings:", err);
    } finally {
      setSaving(false);
    }
  };

  const handleExport = async () => {
    try {
      const json = await invokeBackend<string>("export_settings");
      await navigator.clipboard.writeText(json);
      setExportSuccess(true);
      setTimeout(() => setExportSuccess(false), 2000);
    } catch (err) {
      console.error("Failed to export settings:", err);
    }
  };

  const handleResetToDefaults = () => {
    const defaults = getDefaultSettings();
    setFormSettings(defaults);
    applyTheme(defaults.theme);
    setIsRecordingShortcut(false);
  };

  const refreshWrapperStatus = async () => {
    const status = await invokeBackend<CliWrapperStatus>("get_cli_wrapper_status");
    setWrapperStatus(status);
    return status;
  };

  const handleInstallWrapper = async () => {
    try {
      setWrapperBusy(true);
      setWrapperMessage(null);
      const status = await invokeBackend<CliWrapperStatus>("install_cli_wrapper");
      setWrapperStatus(status);
      setWrapperMessage("CLI wrapper installed");
    } catch (err) {
      setWrapperMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setWrapperBusy(false);
    }
  };

  const handleRemoveWrapper = async () => {
    try {
      setWrapperBusy(true);
      setWrapperMessage(null);
      const status = await invokeBackend<CliWrapperStatus>("remove_cli_wrapper");
      setWrapperStatus(status);
      setWrapperMessage("CLI wrapper removed");
    } catch (err) {
      setWrapperMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setWrapperBusy(false);
      void refreshWrapperStatus().catch(() => undefined);
    }
  };

  const handleGenerateCa = async () => {
    try {
      setCaBusy(true);
      setCaMessage(null);
      const status = await invokeBackend<CaStatus>(
        caStatus?.ready ? "regenerate_proxy_ca" : "generate_proxy_ca"
      );
      setCaStatus(status);
      setCaMessage(caStatus?.ready ? "Local CA regenerated" : "Local CA generated");
    } catch (err) {
      setCaMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setCaBusy(false);
    }
  };

  const handleInstallCaTrust = async () => {
    try {
      setCaBusy(true);
      setCaMessage(null);
      const status = await invokeBackend<CaStatus>("install_proxy_ca_trust");
      setCaStatus(status);
      setFormSettings((prev) => ({ ...prev, proxy_ca_trusted: status.trusted_by_authpilot }));
      setCaMessage("Local CA trusted in macOS Keychain");
    } catch (err) {
      setCaMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setCaBusy(false);
    }
  };

  const handleToggleSystemProxy = async () => {
    try {
      setSystemProxyBusy(true);
      setSystemProxyMessage(null);
      const command = systemProxyStatus?.enabled_from_authpilot
        ? "disable_system_proxy"
        : "enable_system_proxy";
      const status = await invokeBackend<SystemProxyStatus>(command);
      setSystemProxyStatus(status);
      setSystemProxyMessage(
        status.enabled_from_authpilot ? "System proxy enabled" : "System proxy disabled"
      );
    } catch (err) {
      setSystemProxyMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setSystemProxyBusy(false);
    }
  };

  const handleShortcutKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      setIsRecordingShortcut(false);
      return;
    }

    if (event.key === "Backspace" || event.key === "Delete") {
      setFormSettings((prev) => ({
        ...prev,
        dashboard_global_shortcut: "",
      }));
      setIsRecordingShortcut(false);
      return;
    }

    const shortcut = shortcutFromKeyboardEvent(event);
    if (!shortcut) return;

    setFormSettings((prev) => ({
      ...prev,
      dashboard_global_shortcut: shortcut,
    }));
    setIsRecordingShortcut(false);
  };

  const updateAccountThreshold = (accountId: string, threshold: number) => {
    setFormSettings((prev) => ({
      ...prev,
      account_settings: {
        ...prev.account_settings,
        [accountId]: { switch_threshold: threshold },
      },
    }));
  };

  return (
    <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50 p-4">
      <div className="bg-white dark:bg-[#1f1f1f] w-full max-w-lg rounded-[4px] shadow-l2 max-h-[85vh] flex flex-col animate-fade-in-up overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between p-6 pb-4 shrink-0">
          <h2 className="text-lg font-medium text-[#141413] dark:text-[#f3f0ee] tracking-tight">
            Settings
          </h2>
          <button
            onClick={onClose}
            className="h-8 w-8 flex items-center justify-center rounded-[4px] text-[#696969] hover:text-[#141413] dark:hover:text-[#f3f0ee] hover:bg-[#F3F0EE] dark:hover:bg-[#2a2a2a] transition-colors"
          >
            ✕
          </button>
        </div>

        {/* Scrollable Content */}
        <div className="px-6 py-2 space-y-8 overflow-y-auto custom-scrollbar flex-1">
          {/* Global Settings */}
          <div className="space-y-5">
            <div className="eyebrow text-[#696969] dark:text-[#9a9a9a]">Global</div>

            {/* Poll Interval */}
            <div>
              <label className="block text-sm font-medium text-[#141413] dark:text-[#f3f0ee] mb-2">
                Poll Interval
              </label>
              <input
                type="range"
                min="10"
                max="300"
                step="10"
                value={formSettings.poll_interval_seconds}
                onChange={(e) =>
                  setFormSettings((prev) => ({
                    ...prev,
                    poll_interval_seconds: parseInt(e.target.value),
                  }))
                }
                className="w-full accent-[#141413] dark:accent-[#f3f0ee]"
              />
              <div className="text-xs text-[#696969] dark:text-[#9a9a9a] mt-1">
                {formSettings.poll_interval_seconds}s
              </div>
            </div>

            {/* Theme Selector - Custom Pills */}
            <div>
              <label className="block text-sm font-medium text-[#141413] dark:text-[#f3f0ee] mb-2">
                Theme
              </label>
              <div className="flex gap-2">
                {THEMES.map((t) => {
                  const isActive = formSettings.theme === t.value;
                  return (
                    <button
                      key={t.value}
                      onClick={() => {
                        setFormSettings((prev) => ({ ...prev, theme: t.value as "light" | "dark" | "system" }));
                        applyTheme(t.value);
                      }}
                      className={`flex-1 flex items-center justify-center gap-2 px-4 py-2.5 text-sm font-medium rounded-[4px] border transition-all ${
                        isActive
                          ? "bg-[#141413] dark:bg-[#f3f0ee] text-[#F3F0EE] dark:text-[#141413] border-[#141413] dark:border-[#f3f0ee]"
                          : "bg-transparent text-[#141413] dark:text-[#f3f0ee] border-[#D1CDC7] dark:border-[#3a3a3a] hover:border-[#141413] dark:hover:border-[#f3f0ee]"
                      }`}
                    >
                      <span>{t.icon}</span>
                      <span>{t.label}</span>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Toggles */}
            <div className="flex items-center justify-between">
              <label className="text-sm font-medium text-[#141413] dark:text-[#f3f0ee]">
                Auto-switch
              </label>
              <button
                onClick={() =>
                  setFormSettings((prev) => ({
                    ...prev,
                    auto_switch_enabled: !prev.auto_switch_enabled,
                  }))
                }
                className={`relative inline-flex h-6 w-11 items-center rounded-[4px] transition-colors ${
                  formSettings.auto_switch_enabled
                    ? "bg-[#141413] dark:bg-[#f3f0ee]"
                    : "bg-[#D1CDC7] dark:bg-[#3a3a3a]"
                }`}
              >
                <span
                  className={`inline-block h-4 w-4 transform rounded-[4px] bg-white transition-transform ${
                    formSettings.auto_switch_enabled ? "translate-x-6" : "translate-x-1"
                  }`}
                />
              </button>
            </div>

            <div className="flex items-center justify-between">
              <label className="text-sm font-medium text-[#141413] dark:text-[#f3f0ee]">
                Notifications
              </label>
              <button
                onClick={() =>
                  setFormSettings((prev) => ({
                    ...prev,
                    notifications_enabled: !prev.notifications_enabled,
                  }))
                }
                className={`relative inline-flex h-6 w-11 items-center rounded-[4px] transition-colors ${
                  formSettings.notifications_enabled
                    ? "bg-[#141413] dark:bg-[#f3f0ee]"
                    : "bg-[#D1CDC7] dark:bg-[#3a3a3a]"
                }`}
              >
                <span
                  className={`inline-block h-4 w-4 transform rounded-[4px] bg-white transition-transform ${
                    formSettings.notifications_enabled ? "translate-x-6" : "translate-x-1"
                  }`}
                />
              </button>
            </div>

            <div className="flex items-center justify-between gap-4">
              <div>
                <label className="text-sm font-medium text-[#141413] dark:text-[#f3f0ee]">
                  Start at login
                </label>
                <p className="text-xs text-[#696969] dark:text-[#9a9a9a] mt-1">
                  Launch AuthPilot automatically when you sign in to macOS.
                </p>
              </div>
              <button
                onClick={() =>
                  setFormSettings((prev) => ({
                    ...prev,
                    start_at_login: !(prev.start_at_login ?? false),
                  }))
                }
                className={`relative inline-flex h-6 w-11 shrink-0 items-center rounded-[4px] transition-colors ${
                  formSettings.start_at_login
                    ? "bg-[#141413] dark:bg-[#f3f0ee]"
                    : "bg-[#D1CDC7] dark:bg-[#3a3a3a]"
                }`}
              >
                <span
                  className={`inline-block h-4 w-4 transform rounded-[4px] bg-white transition-transform ${
                    formSettings.start_at_login ? "translate-x-6" : "translate-x-1"
                  }`}
                />
              </button>
            </div>

            <div className="flex items-center justify-between gap-4">
              <div>
                <label className="text-sm font-medium text-[#141413] dark:text-[#f3f0ee]">
                  Show in Dock
                </label>
                <p className="text-xs text-[#696969] dark:text-[#9a9a9a] mt-1">
                  Keep AuthPilot visible in the Dock and app switcher.
                </p>
              </div>
              <button
                onClick={() =>
                  setFormSettings((prev) => ({
                    ...prev,
                    show_in_dock: !(prev.show_in_dock ?? false),
                  }))
                }
                className={`relative inline-flex h-6 w-11 shrink-0 items-center rounded-[4px] transition-colors ${
                  formSettings.show_in_dock
                    ? "bg-[#141413] dark:bg-[#f3f0ee]"
                    : "bg-[#D1CDC7] dark:bg-[#3a3a3a]"
                }`}
              >
                <span
                  className={`inline-block h-4 w-4 transform rounded-[4px] bg-white transition-transform ${
                    formSettings.show_in_dock ? "translate-x-6" : "translate-x-1"
                  }`}
                />
              </button>
            </div>

            <div className="space-y-4 rounded-[4px] border border-[#D1CDC7] dark:border-[#3a3a3a] p-4">
              <div className="flex items-center justify-between gap-4">
                <div>
                  <label className="text-sm font-medium text-[#141413] dark:text-[#f3f0ee]">
                    Proxy mode
                  </label>
                  <p className="text-xs text-[#696969] dark:text-[#9a9a9a] mt-1">
                    Route Codex CLI requests through AuthPilot without restarting active sessions.
                  </p>
                </div>
                <button
                  onClick={() =>
                    setFormSettings((prev) => ({
                      ...prev,
                      proxy_mode_enabled: !(prev.proxy_mode_enabled ?? false),
                    }))
                  }
                  className={`relative inline-flex h-6 w-11 shrink-0 items-center rounded-[4px] transition-colors ${
                    formSettings.proxy_mode_enabled
                      ? "bg-[#141413] dark:bg-[#f3f0ee]"
                      : "bg-[#D1CDC7] dark:bg-[#3a3a3a]"
                  }`}
                >
                  <span
                    className={`inline-block h-4 w-4 transform rounded-[4px] bg-white transition-transform ${
                      formSettings.proxy_mode_enabled ? "translate-x-6" : "translate-x-1"
                    }`}
                  />
                </button>
              </div>

              <div>
                <label className="block text-sm font-medium text-[#141413] dark:text-[#f3f0ee] mb-2">
                  Proxy port
                </label>
                <input
                  type="number"
                  min="1024"
                  max="65535"
                  value={formSettings.proxy_port ?? 18080}
                  onChange={(e) =>
                    setFormSettings((prev) => ({
                      ...prev,
                      proxy_port: Math.min(65535, Math.max(1024, parseInt(e.target.value) || 18080)),
                    }))
                  }
                  className="w-full px-4 py-2.5 border border-[#141413]/20 dark:border-[#f3f0ee]/20 rounded-[4px] text-sm focus:outline-none focus:border-[#141413] dark:focus:border-[#f3f0ee] bg-[#F3F0EE] dark:bg-[#2a2a2a] text-[#141413] dark:text-[#f3f0ee]"
                />
              </div>

              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-sm font-medium text-[#141413] dark:text-[#f3f0ee]">
                    CLI wrapper
                  </div>
                  <div className="text-xs text-[#696969] dark:text-[#9a9a9a] mt-1 truncate">
                    {wrapperStatus?.installed ? "Installed" : "Not installed"}
                  </div>
                </div>
                <button
                  type="button"
                  disabled={wrapperBusy}
                  onClick={wrapperStatus?.installed ? handleRemoveWrapper : handleInstallWrapper}
                  className="shrink-0 px-3 py-2 text-sm font-medium rounded-[4px] bg-transparent border border-[#D1CDC7] dark:border-[#3a3a3a] hover:border-[#141413] dark:hover:border-[#f3f0ee] text-[#141413] dark:text-[#f3f0ee] transition-colors disabled:opacity-50"
                >
                  {wrapperBusy ? "Working..." : wrapperStatus?.installed ? "Remove" : "Install"}
                </button>
              </div>

              {wrapperMessage && (
                <div className="text-xs text-[#696969] dark:text-[#9a9a9a] break-words">
                  {wrapperMessage}
                </div>
              )}

              <div className="border-t border-[#D1CDC7] dark:border-[#3a3a3a] pt-4">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-[#141413] dark:text-[#f3f0ee]">
                      Local CA
                    </div>
                    <div className="text-xs text-[#696969] dark:text-[#9a9a9a] mt-1 truncate">
                      {caStatus?.trusted_by_authpilot
                        ? "Trusted in Keychain"
                        : caStatus?.ready
                          ? "Generated"
                          : "Not generated"}
                    </div>
                  </div>
                  <div className="flex shrink-0 gap-2">
                    <button
                      type="button"
                      disabled={caBusy}
                      onClick={handleGenerateCa}
                      className="px-3 py-2 text-sm font-medium rounded-[4px] bg-transparent border border-[#D1CDC7] dark:border-[#3a3a3a] hover:border-[#141413] dark:hover:border-[#f3f0ee] text-[#141413] dark:text-[#f3f0ee] transition-colors disabled:opacity-50"
                    >
                      {caBusy ? "Working..." : caStatus?.ready ? "Regenerate" : "Generate"}
                    </button>
                    <button
                      type="button"
                      disabled={caBusy || !caStatus?.ready || caStatus?.trusted_by_authpilot}
                      onClick={handleInstallCaTrust}
                      className="px-3 py-2 text-sm font-medium rounded-[4px] bg-transparent border border-[#D1CDC7] dark:border-[#3a3a3a] hover:border-[#141413] dark:hover:border-[#f3f0ee] text-[#141413] dark:text-[#f3f0ee] transition-colors disabled:opacity-50"
                    >
                      Trust
                    </button>
                  </div>
                </div>

                {caMessage && (
                  <div className="text-xs text-[#696969] dark:text-[#9a9a9a] break-words mt-2">
                  {caMessage}
                </div>
              )}
            </div>

              <div className="border-t border-[#D1CDC7] dark:border-[#3a3a3a] pt-4">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-[#141413] dark:text-[#f3f0ee]">
                      macOS system proxy
                    </div>
                    <div className="text-xs text-[#696969] dark:text-[#9a9a9a] mt-1 truncate">
                      {!systemProxyStatus?.supported
                        ? "Unsupported"
                        : systemProxyStatus.enabled_from_authpilot
                          ? `${systemProxyStatus.modified_services.length} service${
                              systemProxyStatus.modified_services.length === 1 ? "" : "s"
                            } managed`
                          : "Disabled"}
                    </div>
                  </div>
                  <button
                    type="button"
                    disabled={systemProxyBusy || !systemProxyStatus?.supported}
                    onClick={handleToggleSystemProxy}
                    className="shrink-0 px-3 py-2 text-sm font-medium rounded-[4px] bg-transparent border border-[#D1CDC7] dark:border-[#3a3a3a] hover:border-[#141413] dark:hover:border-[#f3f0ee] text-[#141413] dark:text-[#f3f0ee] transition-colors disabled:opacity-50"
                  >
                    {systemProxyBusy
                      ? "Working..."
                      : systemProxyStatus?.enabled_from_authpilot
                        ? "Disable"
                        : "Enable"}
                  </button>
                </div>

                {systemProxyMessage && (
                  <div className="text-xs text-[#696969] dark:text-[#9a9a9a] break-words mt-2">
                    {systemProxyMessage}
                  </div>
                )}
              </div>
            </div>

            <div>
              <label className="block text-sm font-medium text-[#141413] dark:text-[#f3f0ee] mb-2">
                Usage Bars
              </label>
              <div className="grid grid-cols-2 gap-2">
                {USAGE_DISPLAY_MODES.map((mode) => {
                  const isActive = (formSettings.usage_display_mode ?? "remaining") === mode.value;
                  return (
                    <button
                      key={mode.value}
                      onClick={() =>
                        setFormSettings((prev) => ({
                          ...prev,
                          usage_display_mode: mode.value,
                        }))
                      }
                      className={`text-left px-3 py-2.5 rounded-[4px] border transition-all ${
                        isActive
                          ? "bg-[#141413] dark:bg-[#f3f0ee] text-[#F3F0EE] dark:text-[#141413] border-[#141413] dark:border-[#f3f0ee]"
                          : "bg-transparent text-[#141413] dark:text-[#f3f0ee] border-[#D1CDC7] dark:border-[#3a3a3a] hover:border-[#141413] dark:hover:border-[#f3f0ee]"
                      }`}
                    >
                      <div className="text-sm font-medium">{mode.label}</div>
                      <div className={`text-[11px] mt-1 ${isActive ? "opacity-70" : "text-[#696969] dark:text-[#9a9a9a]"}`}>
                        {mode.description}
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="space-y-3 rounded-[4px] border border-[#D1CDC7] dark:border-[#3a3a3a] p-4">
              <div className="flex items-center justify-between gap-4">
                <div>
                  <label className="text-sm font-medium text-[#141413] dark:text-[#f3f0ee]">
                    Privacy mode
                  </label>
                  <p className="text-xs text-[#696969] dark:text-[#9a9a9a] mt-1">
                    Hide account names and emails in the dashboard and tray.
                  </p>
                </div>
                <button
                  onClick={() =>
                    setFormSettings((prev) => ({
                      ...prev,
                      privacy_mode_enabled: !(prev.privacy_mode_enabled ?? false),
                    }))
                  }
                  className={`relative inline-flex h-6 w-11 shrink-0 items-center rounded-[4px] transition-colors ${
                    formSettings.privacy_mode_enabled
                      ? "bg-[#141413] dark:bg-[#f3f0ee]"
                      : "bg-[#D1CDC7] dark:bg-[#3a3a3a]"
                  }`}
                >
                  <span
                    className={`inline-block h-4 w-4 transform rounded-[4px] bg-white transition-transform ${
                      formSettings.privacy_mode_enabled ? "translate-x-6" : "translate-x-1"
                    }`}
                  />
                </button>
              </div>

              <div>
                <label className="block text-sm font-medium text-[#141413] dark:text-[#f3f0ee] mb-2">
                  Privacy style
                </label>
                <div className="grid grid-cols-2 gap-2">
                  {PRIVACY_MASK_STYLES.map((style) => {
                    const isActive = (formSettings.privacy_mask_style ?? "blur") === style.value;
                    return (
                      <button
                        key={style.value}
                        onClick={() =>
                          setFormSettings((prev) => ({
                            ...prev,
                            privacy_mask_style: style.value,
                          }))
                        }
                        className={`text-left px-3 py-2.5 rounded-[4px] border transition-all ${
                          isActive
                            ? "bg-[#141413] dark:bg-[#f3f0ee] text-[#F3F0EE] dark:text-[#141413] border-[#141413] dark:border-[#f3f0ee]"
                            : "bg-transparent text-[#141413] dark:text-[#f3f0ee] border-[#D1CDC7] dark:border-[#3a3a3a] hover:border-[#141413] dark:hover:border-[#f3f0ee]"
                        }`}
                      >
                        <div className="text-sm font-medium">{style.label}</div>
                        <div className={`text-[11px] mt-1 ${isActive ? "opacity-70" : "text-[#696969] dark:text-[#9a9a9a]"}`}>
                          {style.description}
                        </div>
                      </button>
                    );
                  })}
                </div>
              </div>

              <div>
                <label className="block text-sm font-medium text-[#141413] dark:text-[#f3f0ee] mb-2">
                  Replacement word
                </label>
                <input
                  type="text"
                  value={formSettings.privacy_replacement_text ?? "Hidden"}
                  onChange={(e) =>
                    setFormSettings((prev) => ({
                      ...prev,
                      privacy_replacement_text: e.target.value,
                    }))
                  }
                  placeholder="Hidden"
                  className="w-full px-4 py-2.5 border border-[#141413]/20 dark:border-[#f3f0ee]/20 rounded-[4px] text-sm focus:outline-none focus:border-[#141413] dark:focus:border-[#f3f0ee] bg-[#F3F0EE] dark:bg-[#2a2a2a] text-[#141413] dark:text-[#f3f0ee]"
                />
                <p className="text-xs text-[#696969] dark:text-[#9a9a9a] mt-1">
                  Used when privacy style is set to Replace.
                </p>
              </div>
            </div>

            <div>
              <label className="block text-sm font-medium text-[#141413] dark:text-[#f3f0ee] mb-2">
                Dashboard Shortcut
              </label>
              <button
                type="button"
                onClick={() => setIsRecordingShortcut(true)}
                onBlur={() => setIsRecordingShortcut(false)}
                onKeyDown={handleShortcutKeyDown}
                className={`w-full px-4 py-3 border rounded-[4px] text-left focus:outline-none focus:border-[#141413] dark:focus:border-[#f3f0ee] transition-colors ${
                  isRecordingShortcut
                    ? "border-[#141413] dark:border-[#f3f0ee] bg-white dark:bg-[#262627]"
                    : "border-[#141413]/20 dark:border-[#f3f0ee]/20 bg-[#F3F0EE] dark:bg-[#2a2a2a]"
                }`}
              >
                <span className="block text-xs text-[#696969] dark:text-[#9a9a9a] mb-1">
                  {isRecordingShortcut ? "Press a shortcut" : "Current shortcut"}
                </span>
                <span className="font-mono text-sm text-[#141413] dark:text-[#f3f0ee]">
                  {isRecordingShortcut
                    ? "Waiting for keys..."
                    : formatShortcutLabel(
                        formSettings.dashboard_global_shortcut ?? DEFAULT_DASHBOARD_SHORTCUT
                      )}
                </span>
              </button>
              <p className="text-xs text-[#696969] dark:text-[#9a9a9a] mt-1">
                Click, then press a key combination. Backspace disables it; Escape cancels recording.
              </p>
            </div>

            {/* Cooldown */}
            <div>
              <label className="block text-sm font-medium text-[#141413] dark:text-[#f3f0ee] mb-2">
                Cooldown (seconds)
              </label>
              <input
                type="number"
                min="60"
                max="3600"
                value={formSettings.global_cooldown_seconds}
                onChange={(e) =>
                  setFormSettings((prev) => ({
                    ...prev,
                    global_cooldown_seconds: parseInt(e.target.value) || 300,
                  }))
                }
                className="w-full px-4 py-2.5 border border-[#141413]/20 dark:border-[#f3f0ee]/20 rounded-[4px] text-sm focus:outline-none focus:border-[#141413] dark:focus:border-[#f3f0ee] bg-[#F3F0EE] dark:bg-[#2a2a2a] text-[#141413] dark:text-[#f3f0ee]"
              />
            </div>
          </div>

          {/* Per-Account Thresholds */}
          {accounts.length > 0 && (
            <div className="space-y-5">
              <div className="eyebrow text-[#696969] dark:text-[#9a9a9a]">
                Switch Thresholds
              </div>

              {accounts.map((account) => (
                <div key={account.id} className="flex items-center justify-between">
                  <span className="text-sm text-[#141413] dark:text-[#f3f0ee]">
                    {account.name}
                  </span>
                  <div className="flex items-center gap-2">
                    <input
                      type="range"
                      min="50"
                      max="100"
                      value={
                        formSettings.account_settings[account.id]?.switch_threshold ?? 95
                      }
                      onChange={(e) =>
                        updateAccountThreshold(account.id, parseInt(e.target.value))
                      }
                      className="w-24 accent-[#141413] dark:accent-[#f3f0ee]"
                    />
                    <span className="text-xs text-[#696969] dark:text-[#9a9a9a] w-8 text-right">
                      {formSettings.account_settings[account.id]?.switch_threshold ?? 95}%
                    </span>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Footer Buttons */}
        <div className="flex gap-3 p-6 pt-4 shrink-0 border-t border-[#F3F0EE] dark:border-[#2a2a2a]">
          <button
            onClick={handleResetToDefaults}
            disabled={saving}
            className="px-4 py-2.5 text-sm font-medium rounded-[4px] bg-transparent border border-[#D1CDC7] dark:border-[#3a3a3a] hover:border-[#141413] dark:hover:border-[#f3f0ee] text-[#141413] dark:text-[#f3f0ee] transition-colors disabled:opacity-50"
          >
            Reset
          </button>
          <button
            onClick={onClose}
            className="flex-1 px-5 py-2.5 text-sm font-medium rounded-[4px] bg-[#F3F0EE] hover:bg-[#D1CDC7] dark:bg-[#2a2a2a] dark:hover:bg-[#3a3a3a] text-[#141413] dark:text-[#f3f0ee] transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleExport}
            className="flex-1 px-5 py-2.5 text-sm font-medium rounded-[4px] bg-white border-[1.5px] border-[#141413] dark:border-[#f3f0ee] hover:bg-[#F3F0EE] dark:bg-[#1f1f1f] dark:hover:bg-[#2a2a2a] text-[#141413] dark:text-[#f3f0ee] transition-colors"
          >
            Export
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            className="flex-1 px-5 py-2.5 text-sm font-medium rounded-[4px] bg-[#141413] hover:bg-[#262627] dark:bg-[#f3f0ee] dark:hover:bg-white text-[#F3F0EE] dark:text-[#141413] transition-colors disabled:opacity-50"
          >
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
      </div>

      {/* Export Toast */}
      {exportSuccess && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 px-5 py-3 bg-[#141413] dark:bg-[#f3f0ee] text-[#F3F0EE] dark:text-[#141413] rounded-[4px] shadow-l2 text-sm flex items-center gap-2 z-50 animate-fade-in-up">
          <span className="text-[#F37338]">✓</span> Settings copied
        </div>
      )}
      {saveSuccess && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 px-5 py-3 bg-[#141413] dark:bg-[#f3f0ee] text-[#F3F0EE] dark:text-[#141413] rounded-[4px] shadow-l2 text-sm flex items-center gap-2 z-50 animate-fade-in-up">
          <span className="text-[#F37338]">✓</span> Settings saved
        </div>
      )}
    </div>
  );
}
