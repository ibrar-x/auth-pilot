import { useState, useEffect, useCallback, useMemo } from "react";
import { invokeBackend } from "../lib/platform";
import { getDashboardShortcut } from "../lib/dashboardShortcut";
import { getMaskedText, getPrivacyMaskOptions } from "../lib/privacy";
import { resolveTheme } from "../lib/theme";
import type { AccountInfo, AppSettings, UsageDisplayMode, UsageInfo } from "../types";

interface PopupData {
  active_account: AccountInfo | null;
  accounts: AccountInfo[];
  usages: UsageInfo[];
}

function formatPercent(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  return `${Math.round(value)}%`;
}

function clampPercent(value: number): number {
  return Math.min(Math.max(value, 0), 100);
}

function getDisplayedPercent(
  usedPercent: number | null | undefined,
  displayMode: UsageDisplayMode
): number | null {
  if (usedPercent === null || usedPercent === undefined) return null;
  const used = clampPercent(usedPercent);
  return displayMode === "remaining" ? 100 - used : used;
}

function getUsageColor(
  usedPercent: number | null | undefined,
  displayMode: UsageDisplayMode,
  unavailableColor = "rgba(255,255,255,0.28)"
): string {
  const displayed = getDisplayedPercent(usedPercent, displayMode);
  if (displayed === null) return unavailableColor;

  if (displayMode === "remaining") {
    if (displayed > 40) return "#4ade80";
    if (displayed >= 10) return "#fbbf24";
    return "#f87171";
  }

  if (displayed < 60) return "#4ade80";
  if (displayed < 85) return "#fbbf24";
  return "#f87171";
}

function formatUsageDisplay(
  usedPercent: number | null | undefined,
  displayMode: UsageDisplayMode
): string {
  const displayed = getDisplayedPercent(usedPercent, displayMode);
  if (displayed === null) return "—";
  return `${Math.round(displayed)}% ${displayMode === "remaining" ? "left" : "used"}`;
}

function getRemainingPercent(usage: UsageInfo | undefined): number {
  if (!usage) return 100;
  const primary = usage.primary_used_percent;
  const secondary = usage.secondary_used_percent;
  if (primary === null || primary === undefined) return 0;
  if (secondary === null || secondary === undefined) return 0;
  return Math.max(0, Math.min(100 - primary, 100 - secondary));
}

function formatShortcutLabel(shortcut: string): string {
  if (!shortcut) return "";
  return shortcut
    .replace(/CommandOrControl/gi, "⌘")
    .replace(/Command/gi, "⌘")
    .replace(/Control/gi, "⌃")
    .replace(/Shift/gi, "⇧")
    .replace(/Alt|Option/gi, "⌥")
    .replace(/\+/g, "");
}

const TRAY_COLORS = {
  dark: {
    bg: "#161616",
    border: "rgba(255,255,255,0.1)",
    spinnerTrack: "rgba(255,255,255,0.1)",
    spinner: "rgba(255,255,255,0.72)",
    text: "rgba(255,255,255,0.92)",
    mutedStrong: "rgba(255,255,255,0.72)",
    muted: "rgba(255,255,255,0.46)",
    faint: "rgba(255,255,255,0.28)",
    hairline: "rgba(255,255,255,0.06)",
    rowHover: "rgba(255,255,255,0.05)",
    barTrack: "rgba(255,255,255,0.08)",
    footerHover: "rgba(255,255,255,0.06)",
    planBg: "rgba(139,92,246,0.15)",
    planText: "rgba(139,92,246,0.9)",
    bestBg: "rgba(74,222,128,0.12)",
    bestBorder: "rgba(74,222,128,0.28)",
    bestText: "#7df29a",
  },
  light: {
    bg: "#FCFBFA",
    border: "rgba(20,20,19,0.12)",
    spinnerTrack: "rgba(20,20,19,0.12)",
    spinner: "rgba(20,20,19,0.72)",
    text: "rgba(20,20,19,0.92)",
    mutedStrong: "rgba(20,20,19,0.68)",
    muted: "rgba(20,20,19,0.48)",
    faint: "rgba(20,20,19,0.28)",
    hairline: "rgba(20,20,19,0.08)",
    rowHover: "rgba(20,20,19,0.05)",
    barTrack: "rgba(20,20,19,0.1)",
    footerHover: "rgba(20,20,19,0.06)",
    planBg: "rgba(124,58,237,0.1)",
    planText: "rgba(109,40,217,0.92)",
    bestBg: "rgba(22,163,74,0.1)",
    bestBorder: "rgba(22,163,74,0.24)",
    bestText: "#15803d",
  },
} as const;

export function TrayPopup() {
  const [data, setData] = useState<PopupData | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [usageDisplayMode, setUsageDisplayMode] = useState<UsageDisplayMode>("remaining");
  const [loading, setLoading] = useState(true);
  const [switchingId, setSwitchingId] = useState<string | null>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [systemTheme, setSystemTheme] = useState<"light" | "dark">(() => resolveTheme("system"));

  const fetchData = useCallback(async () => {
    try {
      const [result, settings] = await Promise.all([
        invokeBackend<PopupData>("get_tray_popup_data"),
        invokeBackend<AppSettings>("get_settings").catch(() => null),
      ]);
      setData(result);
      setSettings(settings);
      setUsageDisplayMode(settings?.usage_display_mode ?? "remaining");
    } catch (err) {
      console.error("Failed to fetch popup data:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  const markPopupInteraction = useCallback(() => {
    invokeBackend("tray_popup_interaction").catch(() => {});
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000);
    return () => clearInterval(interval);
  }, [fetchData]);

  useEffect(() => {
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => setSystemTheme(resolveTheme("system"));
    mql.addEventListener("change", handleChange);
    return () => mql.removeEventListener("change", handleChange);
  }, []);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;

    import("@tauri-apps/api/event")
      .then(async ({ listen }) => {
        const accountUnlisten = await listen("account-switched", () => {
          fetchData();
        });
        const settingsUnlisten = await listen<AppSettings>("settings-updated", (event) => {
          setSettings(event.payload);
          setUsageDisplayMode(event.payload.usage_display_mode ?? "remaining");
        });

        if (disposed) {
          accountUnlisten();
          settingsUnlisten();
        } else {
          unlisteners.push(accountUnlisten, settingsUnlisten);
        }
      })
      .catch((err) => {
        console.error("Failed to listen for account switch events:", err);
      });

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [fetchData]);

  const activeUsage = useMemo(() => {
    if (!data?.active_account) return undefined;
    return data.usages.find((u) => u.account_id === data.active_account!.id);
  }, [data]);

  const sortedAccounts = useMemo(() => {
    if (!data) return [];
    return [...data.accounts]
      .filter((a) => !a.is_active)
      .sort((a, b) => {
        const ua = data.usages.find((u) => u.account_id === a.id);
        const ub = data.usages.find((u) => u.account_id === b.id);
        return getRemainingPercent(ub) - getRemainingPercent(ua);
      });
  }, [data]);

  const bestAccountId = useMemo(() => {
    if (sortedAccounts.length === 0) return null;
    return sortedAccounts[0].id;
  }, [sortedAccounts]);

  const handleSwitch = async (accountId: string) => {
    try {
      setSwitchingId(accountId);
      await invokeBackend("popup_switch_account", { accountId });
      setData((current) => {
        if (!current) return current;
        const activeAccount = current.accounts.find((account) => account.id === accountId);
        return {
          ...current,
          active_account: activeAccount ? { ...activeAccount, is_active: true } : current.active_account,
          accounts: current.accounts.map((account) => ({
            ...account,
            is_active: account.id === accountId,
          })),
        };
      });
      await fetchData();
    } catch (err) {
      console.error("Switch failed:", err);
    } finally {
      setSwitchingId(null);
    }
  };

  const handleOpenDashboard = () => {
    invokeBackend("show_main_window").catch(() => {});
  };

  const handleRefresh = async () => {
    setIsRefreshing(true);
    try {
      await invokeBackend("refresh_all_accounts_usage");
      await fetchData();
    } catch (err) {
      console.error("Refresh failed:", err);
    } finally {
      setIsRefreshing(false);
    }
  };

  const handleQuit = () => {
    invokeBackend("quit_app").catch(() => {});
  };

  const handleOpenSettings = () => {
    invokeBackend("open_settings").catch(() => {});
  };

  const handleTogglePrivacy = async () => {
    if (!settings) return;

    const nextSettings = {
      ...settings,
      privacy_mode_enabled: !(settings.privacy_mode_enabled ?? false),
    };

    setSettings(nextSettings);
    try {
      await invokeBackend("save_settings", { newSettings: nextSettings });
    } catch (err) {
      setSettings(settings);
      console.error("Failed to toggle privacy mode:", err);
    }
  };

  const resolvedTrayTheme =
    settings?.theme === "light" || settings?.theme === "dark" ? settings.theme : systemTheme;
  const colors = TRAY_COLORS[resolvedTrayTheme];

  if (loading || !data) {
    return (
      <div style={{ width: "100%", height: "100%", background: colors.bg, borderRadius: 4, border: `0.5px solid ${colors.border}`, display: "flex", alignItems: "center", justifyContent: "center", fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif", boxSizing: "border-box", overflow: "hidden" }}>
        <div style={{ width: 16, height: 16, border: `2px solid ${colors.spinnerTrack}`, borderTopColor: colors.spinner, borderRadius: "50%", animation: "spin 0.8s linear infinite" }} />
        <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
      </div>
    );
  }

  const privacyMask = getPrivacyMaskOptions(settings);
  const dashboardShortcutLabel = formatShortcutLabel(getDashboardShortcut(settings));
  const activeAccountName = data.active_account
    ? getMaskedText(data.active_account.name, privacyMask)
    : null;

  return (
    <div style={{ width: "100%", height: "100%", background: colors.bg, borderRadius: 4, border: `0.5px solid ${colors.border}`, fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif", overflow: "hidden", userSelect: "none", display: "flex", flexDirection: "column", boxSizing: "border-box" }} onPointerDownCapture={markPopupInteraction} onMouseDown={(e) => e.stopPropagation()}>
      {data.active_account && (
        <div style={{ padding: "14px 14px 10px", flexShrink: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 6 }}>
            <span style={{ width: 6, height: 6, borderRadius: "50%", background: "#4ade80", display: "inline-block", animation: "pulse 2s ease-in-out infinite" }} />
            <span style={{ fontSize: 10, fontWeight: 600, letterSpacing: "0.06em", color: colors.mutedStrong, textTransform: "uppercase" }}>Active</span>
            {data.active_account.plan_type && (
              <span style={{ fontSize: 9, fontWeight: 500, padding: "1px 6px", borderRadius: 4, background: colors.planBg, color: colors.planText, textTransform: "uppercase", letterSpacing: "0.03em" }}>{data.active_account.plan_type}</span>
            )}
          </div>
          <div style={{ fontSize: 14, fontWeight: 500, color: colors.text, marginBottom: 10, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", filter: activeAccountName?.blur ? "blur(4px)" : undefined }}>{activeAccountName?.text}</div>
          <div style={{ display: "flex", gap: 10 }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 9, fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", color: colors.faint, marginBottom: 3, textTransform: "uppercase", letterSpacing: "0.03em" }}>5h <span style={{ color: colors.mutedStrong }}>{formatUsageDisplay(activeUsage?.primary_used_percent, usageDisplayMode)}</span></div>
              <div style={{ height: 3, background: colors.barTrack, borderRadius: 2, overflow: "hidden" }}>
                <div style={{ height: "100%", width: `${getDisplayedPercent(activeUsage?.primary_used_percent, usageDisplayMode) ?? 0}%`, background: getUsageColor(activeUsage?.primary_used_percent, usageDisplayMode, colors.faint), borderRadius: 2, transition: "width 0.3s ease" }} />
              </div>
            </div>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 9, fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", color: colors.faint, marginBottom: 3, textTransform: "uppercase", letterSpacing: "0.03em" }}>7d <span style={{ color: colors.mutedStrong }}>{formatUsageDisplay(activeUsage?.secondary_used_percent, usageDisplayMode)}</span></div>
              <div style={{ height: 3, background: colors.barTrack, borderRadius: 2, overflow: "hidden" }}>
                <div style={{ height: "100%", width: `${getDisplayedPercent(activeUsage?.secondary_used_percent, usageDisplayMode) ?? 0}%`, background: getUsageColor(activeUsage?.secondary_used_percent, usageDisplayMode, colors.faint), borderRadius: 2, transition: "width 0.3s ease" }} />
              </div>
            </div>
          </div>
        </div>
      )}

      <div style={{ height: 1, background: colors.hairline, margin: "0 14px", flexShrink: 0 }} />

      <div style={{ padding: "4px 0", flex: 1, minHeight: 0, overflowY: "auto", overflowX: "hidden" }}>
        {sortedAccounts.map((account) => {
          const usage = data.usages.find((u) => u.account_id === account.id);
          const isHovered = hoveredId === account.id;
          const isSwitching = switchingId === account.id;
          const isBest = account.id === bestAccountId;
          const primaryUsed = usage?.primary_used_percent;
          const displayedPrimary = getDisplayedPercent(primaryUsed, usageDisplayMode);
          const accountName = getMaskedText(account.name, privacyMask);

          return (
            <div key={account.id} onMouseEnter={() => setHoveredId(account.id)} onMouseLeave={() => setHoveredId(null)} onClick={() => !isSwitching && handleSwitch(account.id)}
              style={{ display: "grid", gridTemplateColumns: "minmax(0, 1fr) 108px", alignItems: "center", columnGap: 12, height: 34, padding: "0 14px", margin: "3px 6px", borderRadius: 4, cursor: isSwitching ? "wait" : "pointer", background: isHovered ? colors.rowHover : "transparent", transition: "background 0.12s ease", opacity: isSwitching ? 0.5 : 1 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
                <span style={{ width: 5, height: 5, borderRadius: "50%", background: getUsageColor(primaryUsed, usageDisplayMode, colors.faint), flexShrink: 0 }} />
                <span style={{ fontSize: 12, color: colors.text, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", filter: accountName.blur ? "blur(4px)" : undefined }}>{accountName.text}</span>
                {isBest && <span style={{ fontSize: 8, fontWeight: 700, padding: "1px 6px", borderRadius: 999, background: colors.bestBg, border: `0.5px solid ${colors.bestBorder}`, color: colors.bestText, textTransform: "uppercase", letterSpacing: "0.06em", flexShrink: 0 }}>best</span>}
              </div>
              <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 8, minWidth: 0 }}>
                {!isHovered ? (
                  <>
                    <div style={{ width: 54, height: 3, background: colors.barTrack, borderRadius: 2, overflow: "hidden" }}>
                      <div style={{ height: "100%", width: `${displayedPrimary ?? 0}%`, background: getUsageColor(primaryUsed, usageDisplayMode, colors.faint), borderRadius: 2 }} />
                    </div>
                    <span style={{ fontSize: 10, fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", color: colors.mutedStrong, width: 40, textAlign: "right" }}>{formatPercent(displayedPrimary)}</span>
                  </>
                ) : (
                  <span style={{ fontSize: 10, fontWeight: 500, color: colors.faint, letterSpacing: "0.02em" }}>switch</span>
                )}
              </div>
            </div>
          );
        })}
      </div>

      <div style={{ height: 1, background: colors.hairline, margin: "0 14px", flexShrink: 0 }} />

      <div style={{ display: "grid", gridTemplateColumns: "repeat(5, minmax(0, 1fr))", padding: "6px 8px", gap: 3, flexShrink: 0 }}>
        <FooterButton colors={colors} label="Dash" shortcut={dashboardShortcutLabel} icon={<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /></svg>} onClick={handleOpenDashboard} title="Dashboard" />
        <FooterButton colors={colors} label="Settings" icon={<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" /></svg>} onClick={handleOpenSettings} title="Settings" />
        <FooterButton colors={colors} label="Privacy" icon={privacyMask.enabled ? <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M17.94 17.94A10.94 10.94 0 0 1 12 20C7 20 2.73 16.89 1 12.5a11.7 11.7 0 0 1 3.07-4.56" /><path d="M9.9 4.24A10.64 10.64 0 0 1 12 4c5 0 9.27 3.11 11 7.5a11.7 11.7 0 0 1-2.11 3.19" /><path d="M14.12 14.12a3 3 0 0 1-4.24-4.24" /><path d="M3 3l18 18" /></svg> : <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M2 12.5C3.73 8.11 8 5 12 5s8.27 3.11 10 7.5C20.27 16.89 16 20 12 20S3.73 16.89 2 12.5z" /><circle cx="12" cy="12.5" r="3" /></svg>} onClick={handleTogglePrivacy} title={privacyMask.enabled ? "Show details" : "Hide details"} disabled={!settings} />
        <FooterButton colors={colors} label="Refresh" icon={isRefreshing ? <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ animation: "spin 0.8s linear infinite" }}><path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" /></svg> : <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" /></svg>} onClick={handleRefresh} title="Refresh" disabled={isRefreshing} />
        <FooterButton colors={colors} label="Quit" icon={<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M18 6L6 18M6 6l12 12" /></svg>} onClick={handleQuit} danger title="Quit" />
      </div>

      <style>{`@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } } @keyframes spin { to { transform: rotate(360deg); } }`}</style>
    </div>
  );
}

type TrayColors = (typeof TRAY_COLORS)[keyof typeof TRAY_COLORS];

function FooterButton({ colors, icon, label, shortcut, onClick, danger, title, disabled }: { colors: TrayColors; icon: React.ReactNode; label: string; shortcut?: string; onClick: () => void; danger?: boolean; title: string; disabled?: boolean }) {
  const [hovered, setHovered] = useState(false);
  const handleClick = (event: React.MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    if (!disabled) onClick();
  };

  return (
    <button onPointerDown={(event) => event.stopPropagation()} onMouseEnter={() => !disabled && setHovered(true)} onMouseLeave={() => setHovered(false)} onClick={handleClick} title={title} disabled={disabled}
      style={{ minWidth: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 2, height: 44, borderRadius: 4, border: "none", background: hovered && !disabled ? (danger ? "rgba(248,113,113,0.12)" : colors.footerHover) : "transparent", color: hovered && !disabled ? (danger ? "#f87171" : colors.text) : disabled ? colors.faint : colors.mutedStrong, cursor: disabled ? "not-allowed" : "pointer", transition: "all 0.12s ease", padding: "3px 2px" }}>
      {icon}
      <span style={{ maxWidth: "100%", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 8.5, lineHeight: 1, color: colors.muted }}>{label}</span>
      {shortcut && <span style={{ maxWidth: "100%", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 8, lineHeight: 1, fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", color: colors.faint }}>{shortcut}</span>}
    </button>
  );
}
