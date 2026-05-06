import { useState } from "react";
import type { AppSettings, AccountWithUsage } from "../types";
import { invokeBackend } from "../lib/platform";

interface SettingsProps {
  settings: AppSettings;
  onSave: (settings: AppSettings) => Promise<void>;
  onClose: () => void;
  accounts: AccountWithUsage[];
}

function applyTheme(theme: string) {
  const root = document.documentElement;
  if (theme === "dark") {
    root.classList.add("dark");
  } else if (theme === "light") {
    root.classList.remove("dark");
  } else {
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    if (mql.matches) {
      root.classList.add("dark");
    } else {
      root.classList.remove("dark");
    }
  }
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

export function Settings({ settings, onSave, onClose, accounts }: SettingsProps) {
  const [formSettings, setFormSettings] = useState<AppSettings>({ ...settings });
  const [saving, setSaving] = useState(false);
  const [exportSuccess, setExportSuccess] = useState(false);

  const handleSave = async () => {
    try {
      setSaving(true);
      await onSave(formSettings);
      applyTheme(formSettings.theme);
      onClose();
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
    </div>
  );
}
