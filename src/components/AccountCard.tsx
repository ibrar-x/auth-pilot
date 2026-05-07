import { useState, useRef, useEffect } from "react";
import type { AccountWithUsage, UsageDisplayMode } from "../types";
import { getMaskedText, type PrivacyMaskOptions } from "../lib/privacy";
import { UsageBar } from "./UsageBar";

interface AccountCardProps {
  account: AccountWithUsage;
  onSwitch: () => void;
  onDelete: () => void;
  onRefresh: () => Promise<void>;
  onRename: (newName: string) => Promise<void>;
  switching?: boolean;
  switchDisabled?: boolean;
  masked?: boolean;
  onToggleMask?: () => void;
  usageDisplayMode?: UsageDisplayMode;
  privacyMask?: PrivacyMaskOptions;
}

function formatLastRefresh(date: Date | null): string {
  if (!date) return "Never";
  const now = new Date();
  const diff = Math.floor((now.getTime() - date.getTime()) / 1000);
  if (diff < 5) return "Just now";
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return date.toLocaleDateString();
}

function BlurredText({ children, blur }: { children: React.ReactNode; blur: boolean }) {
  return (
    <span
      className={`transition-all duration-200 select-none ${blur ? "blur-sm" : ""}`}
      style={blur ? { userSelect: "none" } : undefined}
    >
      {children}
    </span>
  );
}

export function AccountCard({
  account,
  onSwitch,
  onDelete,
  onRefresh,
  onRename,
  switching,
  switchDisabled,
  masked = false,
  onToggleMask,
  usageDisplayMode = "remaining",
  privacyMask,
}: AccountCardProps) {
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(
    account.usage && !account.usage.error ? new Date() : null
  );
  const [isEditing, setIsEditing] = useState(false);
  const [editName, setEditName] = useState(account.name);
  const [switchError, setSwitchError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditing]);

  const handleRefresh = async () => {
    setIsRefreshing(true);
    try {
      await onRefresh();
      setLastRefresh(new Date());
    } catch (err) {
      console.error("Failed to refresh usage:", err);
    } finally {
      setIsRefreshing(false);
    }
  };

  const handleSwitch = async () => {
    setSwitchError(null);
    try {
      await onSwitch();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setSwitchError(message);
      console.error("Failed to switch account:", err);
    }
  };

  const handleRename = async () => {
    const trimmed = editName.trim();
    if (trimmed && trimmed !== account.name) {
      try {
        await onRename(trimmed);
      } catch {
        setEditName(account.name);
      }
    } else {
      setEditName(account.name);
    }
    setIsEditing(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleRename();
    } else if (e.key === "Escape") {
      setEditName(account.name);
      setIsEditing(false);
    }
  };

  const planDisplay = account.plan_type
    ? account.plan_type.charAt(0).toUpperCase() + account.plan_type.slice(1)
    : account.auth_mode === "api_key"
      ? "API Key"
      : "Unknown";

  const planColors: Record<string, string> = {
    pro: "bg-[#F3F0EE] text-[#141413] border-[#D1CDC7] dark:bg-[#2a2a2a] dark:text-[#f3f0ee] dark:border-[#3a3a3a]",
    plus: "bg-[#F3F0EE] text-[#141413] border-[#D1CDC7] dark:bg-[#2a2a2a] dark:text-[#f3f0ee] dark:border-[#3a3a3a]",
    team: "bg-[#F3F0EE] text-[#141413] border-[#D1CDC7] dark:bg-[#2a2a2a] dark:text-[#f3f0ee] dark:border-[#3a3a3a]",
    enterprise: "bg-[#F3F0EE] text-[#141413] border-[#D1CDC7] dark:bg-[#2a2a2a] dark:text-[#f3f0ee] dark:border-[#3a3a3a]",
    free: "bg-[#F3F0EE] text-[#696969] border-[#D1CDC7] dark:bg-[#2a2a2a] dark:text-[#9a9a9a] dark:border-[#3a3a3a]",
    api_key: "bg-[#F3F0EE] text-[#F37338] border-[#F37338]/30 dark:bg-[#2a2a2a] dark:text-[#F37338] dark:border-[#F37338]/30",
  };

  const planKey = account.plan_type?.toLowerCase() || "api_key";
  const planColorClass = planColors[planKey] || planColors.free;
  const effectivePrivacyMask: PrivacyMaskOptions = privacyMask?.enabled
    ? privacyMask
    : { enabled: masked, style: "blur", replacementText: "Hidden" };
  const displayedName = getMaskedText(account.name, effectivePrivacyMask);
  const displayedEmail = account.email ? getMaskedText(account.email, effectivePrivacyMask) : null;
  const detailsHidden = effectivePrivacyMask.enabled;
  const globalPrivacyActive = privacyMask?.enabled ?? false;

  return (
    <div
      className={`relative bg-white dark:bg-[#1f1f1f] rounded-[4px] p-8 transition-all duration-200 ${
        account.is_active
          ? "shadow-l2 ring-1 ring-[#F37338]/30"
          : "shadow-l2 hover:shadow-[rgba(0,0,0,0.12)_0px_32px_64px_0px]"
      }`}
    >
      <div className="flex items-start justify-between mb-4">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-1">
            {account.is_active && (
              <span className="flex h-2 w-2 mr-1">
                <span className="animate-ping absolute inline-flex h-2 w-2 rounded-full bg-[#F37338] opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-[#F37338]"></span>
              </span>
            )}
            {isEditing ? (
              <input
                ref={inputRef}
                type="text"
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
                onBlur={handleRename}
                onKeyDown={handleKeyDown}
                className="font-medium text-[#141413] dark:text-[#f3f0ee] bg-[#F3F0EE] dark:bg-[#2a2a2a] px-3 py-1 rounded-[4px] border border-[#D1CDC7] dark:border-[#3a3a3a] focus:outline-none focus:border-[#141413] dark:focus:border-[#f3f0ee] w-full text-base"
              />
            ) : (
              <h3
                className="font-medium text-[#141413] dark:text-[#f3f0ee] truncate cursor-pointer hover:text-[#696969] dark:hover:text-[#9a9a9a] transition-colors text-base"
                onClick={() => {
                  if (detailsHidden) return;
                  setEditName(account.name);
                  setIsEditing(true);
                }}
                title={detailsHidden ? undefined : "Click to rename"}
              >
                <BlurredText blur={displayedName.blur}>{displayedName.text}</BlurredText>
              </h3>
            )}
          </div>
          {displayedEmail && (
            <p className="text-sm text-[#696969] dark:text-[#9a9a9a] truncate">
              <BlurredText blur={displayedEmail.blur}>{displayedEmail.text}</BlurredText>
            </p>
          )}
        </div>

        <div className="flex items-center gap-2">
          {onToggleMask && (
            <button
              onClick={onToggleMask}
              className="p-2 text-[#696969] hover:text-[#141413] dark:hover:text-[#f3f0ee] transition-colors rounded-[4px] hover:bg-[#F3F0EE] dark:hover:bg-[#2a2a2a]"
              title={globalPrivacyActive ? "Global privacy is active" : masked ? "Show info" : "Hide info"}
            >
              {detailsHidden ? (
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
                </svg>
              ) : (
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                </svg>
              )}
            </button>
          )}
          <span className={`px-3 py-1 text-xs font-medium rounded-[4px] border ${planColorClass}`}>
            {planDisplay}
          </span>
        </div>
      </div>

      <div className="mb-4">
        <UsageBar
          usage={account.usage}
          loading={isRefreshing || account.usageLoading}
          displayMode={usageDisplayMode}
        />
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 text-xs mb-5">
        <div className="text-[#D1CDC7] dark:text-[#696969]">
          Last updated: {formatLastRefresh(lastRefresh)}
        </div>
      </div>

      <div className="flex gap-2">
        {account.is_active ? (
          <button
            disabled
            className="flex-1 px-4 py-2 text-sm font-medium rounded-[4px] bg-[#F3F0EE] dark:bg-[#2a2a2a] text-[#D1CDC7] dark:text-[#696969] border border-[#D1CDC7] dark:border-[#3a3a3a] cursor-default"
          >
            Active
          </button>
        ) : (
          <button
            onClick={handleSwitch}
            disabled={switching || switchDisabled}
            className={`flex-1 px-4 py-2 text-sm font-medium rounded-[4px] transition-colors disabled:opacity-50 ${
              switchDisabled
                ? "bg-[#F3F0EE] dark:bg-[#2a2a2a] text-[#D1CDC7] dark:text-[#696969] cursor-not-allowed"
                : "bg-[#141413] hover:bg-[#262627] dark:bg-[#f3f0ee] dark:hover:bg-white text-[#F3F0EE] dark:text-[#141413]"
            }`}
          >
            {switching ? "Switching..." : "Switch"}
          </button>
        )}
        <button
          onClick={handleRefresh}
          disabled={isRefreshing}
          className="px-4 py-2 text-sm rounded-[4px] bg-[#F3F0EE] hover:bg-[#D1CDC7] dark:bg-[#2a2a2a] dark:hover:bg-[#3a3a3a] text-[#141413] dark:text-[#f3f0ee] transition-colors"
          title="Refresh usage"
        >
          {isRefreshing ? (
            <svg className="animate-spin h-4 w-4" viewBox="0 0 24 24" fill="none">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
          ) : (
            <span>↻</span>
          )}
        </button>
        <button
          onClick={onDelete}
          className="px-4 py-2 text-sm rounded-[4px] bg-[#CF4500] hover:bg-[#b33c00] text-white transition-colors"
          title="Remove account"
        >
          ✕
        </button>
      </div>

      {switchError && (
        <div className="mt-3 px-3 py-2 bg-[#F3F0EE] dark:bg-[#2a2a2a] border border-[#CF4500] rounded-[4px] text-xs text-[#CF4500]">
          Switch failed: {switchError}
        </div>
      )}
    </div>
  );
}
