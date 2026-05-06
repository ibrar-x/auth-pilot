import { useState, useEffect, useCallback, useMemo } from "react";
import { invokeBackend } from "../lib/platform";
import type { AccountInfo, UsageInfo } from "../types";

interface PopupData {
  active_account: AccountInfo | null;
  accounts: AccountInfo[];
  usages: UsageInfo[];
}

function getUsageColor(percent: number | null | undefined): string {
  if (percent === null || percent === undefined) return "rgba(255,255,255,0.28)";
  if (percent < 60) return "#4ade80";
  if (percent < 85) return "#fbbf24";
  return "#f87171";
}

function getStatusColor(percent: number | null | undefined): string {
  if (percent === null || percent === undefined) return "rgba(255,255,255,0.28)";
  if (percent < 60) return "#4ade80";
  if (percent < 85) return "#fbbf24";
  return "#f87171";
}

function formatPercent(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  return `${Math.round(value)}%`;
}

function getRemainingPercent(usage: UsageInfo | undefined): number {
  if (!usage) return 100;
  const p = usage.primary_used_percent;
  if (p === null || p === undefined) return 100;
  return Math.max(0, 100 - p);
}

export function TrayPopup() {
  const [data, setData] = useState<PopupData | null>(null);
  const [loading, setLoading] = useState(true);
  const [switchingId, setSwitchingId] = useState<string | null>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);

  const fetchData = useCallback(async () => {
    try {
      const result = await invokeBackend<PopupData>("get_tray_popup_data");
      setData(result);
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

  if (loading || !data) {
    return (
      <div style={{ width: "100%", height: "100%", background: "#161616", borderRadius: 4, border: "0.5px solid rgba(255,255,255,0.1)", display: "flex", alignItems: "center", justifyContent: "center", fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif", boxSizing: "border-box", overflow: "hidden" }}>
        <div style={{ width: 16, height: 16, border: "2px solid rgba(255,255,255,0.1)", borderTopColor: "rgba(255,255,255,0.72)", borderRadius: "50%", animation: "spin 0.8s linear infinite" }} />
        <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
      </div>
    );
  }

  return (
    <div style={{ width: "100%", height: "100%", background: "#161616", borderRadius: 4, border: "0.5px solid rgba(255,255,255,0.1)", fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif", overflow: "hidden", userSelect: "none", display: "flex", flexDirection: "column", boxSizing: "border-box" }} onPointerDownCapture={markPopupInteraction} onMouseDown={(e) => e.stopPropagation()}>
      {data.active_account && (
        <div style={{ padding: "14px 14px 10px", flexShrink: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 6 }}>
            <span style={{ width: 6, height: 6, borderRadius: "50%", background: "#4ade80", display: "inline-block", animation: "pulse 2s ease-in-out infinite" }} />
            <span style={{ fontSize: 10, fontWeight: 600, letterSpacing: "0.06em", color: "rgba(255,255,255,0.72)", textTransform: "uppercase" }}>Active</span>
            {data.active_account.plan_type && (
              <span style={{ fontSize: 9, fontWeight: 500, padding: "1px 6px", borderRadius: 4, background: "rgba(139,92,246,0.15)", color: "rgba(139,92,246,0.9)", textTransform: "uppercase", letterSpacing: "0.03em" }}>{data.active_account.plan_type}</span>
            )}
          </div>
          <div style={{ fontSize: 14, fontWeight: 500, color: "rgba(255,255,255,0.92)", marginBottom: 10, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{data.active_account.name}</div>
          <div style={{ display: "flex", gap: 10 }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 9, fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", color: "rgba(255,255,255,0.28)", marginBottom: 3, textTransform: "uppercase", letterSpacing: "0.03em" }}>5h <span style={{ color: "rgba(255,255,255,0.72)" }}>{formatPercent(activeUsage?.primary_used_percent)}</span></div>
              <div style={{ height: 3, background: "rgba(255,255,255,0.08)", borderRadius: 2, overflow: "hidden" }}>
                <div style={{ height: "100%", width: `${activeUsage?.primary_used_percent ?? 0}%`, background: getUsageColor(activeUsage?.primary_used_percent), borderRadius: 2, transition: "width 0.3s ease" }} />
              </div>
            </div>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 9, fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", color: "rgba(255,255,255,0.28)", marginBottom: 3, textTransform: "uppercase", letterSpacing: "0.03em" }}>7d <span style={{ color: "rgba(255,255,255,0.72)" }}>{formatPercent(activeUsage?.secondary_used_percent)}</span></div>
              <div style={{ height: 3, background: "rgba(255,255,255,0.08)", borderRadius: 2, overflow: "hidden" }}>
                <div style={{ height: "100%", width: `${activeUsage?.secondary_used_percent ?? 0}%`, background: getUsageColor(activeUsage?.secondary_used_percent), borderRadius: 2, transition: "width 0.3s ease" }} />
              </div>
            </div>
          </div>
        </div>
      )}

      <div style={{ height: 1, background: "rgba(255,255,255,0.06)", margin: "0 14px", flexShrink: 0 }} />

      <div style={{ padding: "4px 0", flex: 1, minHeight: 0, overflowY: "auto", overflowX: "hidden" }}>
        {sortedAccounts.map((account) => {
          const usage = data.usages.find((u) => u.account_id === account.id);
          const isHovered = hoveredId === account.id;
          const isSwitching = switchingId === account.id;
          const isBest = account.id === bestAccountId;
          const primary = usage?.primary_used_percent ?? null;

          return (
            <div key={account.id} onMouseEnter={() => setHoveredId(account.id)} onMouseLeave={() => setHoveredId(null)} onClick={() => !isSwitching && handleSwitch(account.id)}
              style={{ display: "grid", gridTemplateColumns: "minmax(0, 1fr) 108px", alignItems: "center", columnGap: 12, height: 34, padding: "0 14px", margin: "3px 6px", borderRadius: 4, cursor: isSwitching ? "wait" : "pointer", background: isHovered ? "rgba(255,255,255,0.05)" : "transparent", transition: "background 0.12s ease", opacity: isSwitching ? 0.5 : 1 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
                <span style={{ width: 5, height: 5, borderRadius: "50%", background: getStatusColor(primary), flexShrink: 0 }} />
                <span style={{ fontSize: 12, color: "rgba(255,255,255,0.92)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{account.name}</span>
                {isBest && <span style={{ fontSize: 8, fontWeight: 700, padding: "1px 6px", borderRadius: 999, background: "rgba(74,222,128,0.12)", border: "0.5px solid rgba(74,222,128,0.28)", color: "#7df29a", textTransform: "uppercase", letterSpacing: "0.06em", flexShrink: 0 }}>best</span>}
              </div>
              <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 8, minWidth: 0 }}>
                {!isHovered ? (
                  <>
                    <div style={{ width: 64, height: 3, background: "rgba(255,255,255,0.08)", borderRadius: 2, overflow: "hidden" }}>
                      <div style={{ height: "100%", width: `${primary ?? 0}%`, background: getUsageColor(primary), borderRadius: 2 }} />
                    </div>
                    <span style={{ fontSize: 10, fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", color: "rgba(255,255,255,0.72)", width: 30, textAlign: "right" }}>{formatPercent(primary)}</span>
                  </>
                ) : (
                  <span style={{ fontSize: 10, fontWeight: 500, color: "rgba(255,255,255,0.28)", letterSpacing: "0.02em" }}>switch</span>
                )}
              </div>
            </div>
          );
        })}
      </div>

      <div style={{ height: 1, background: "rgba(255,255,255,0.06)", margin: "0 14px", flexShrink: 0 }} />

      <div style={{ display: "flex", padding: "6px 8px", gap: 2, flexShrink: 0 }}>
        <FooterButton icon={<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /></svg>} shortcut="D" onClick={handleOpenDashboard} title="Dashboard" />
        <FooterButton icon={<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" /></svg>} shortcut="," onClick={handleOpenSettings} title="Settings" />
        <FooterButton icon={isRefreshing ? <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ animation: "spin 0.8s linear infinite" }}><path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" /></svg> : <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" /></svg>} onClick={handleRefresh} title="Refresh" disabled={isRefreshing} />
        <FooterButton icon={<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M18 6L6 18M6 6l12 12" /></svg>} onClick={handleQuit} danger title="Quit" />
      </div>

      <style>{`@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } } @keyframes spin { to { transform: rotate(360deg); } }`}</style>
    </div>
  );
}

function FooterButton({ icon, shortcut, onClick, danger, title, disabled }: { icon: React.ReactNode; shortcut?: string; onClick: () => void; danger?: boolean; title: string; disabled?: boolean }) {
  const [hovered, setHovered] = useState(false);
  return (
    <button onMouseEnter={() => !disabled && setHovered(true)} onMouseLeave={() => setHovered(false)} onClick={onClick} title={title} disabled={disabled}
      style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 2, height: 36, borderRadius: 4, border: "none", background: hovered && !disabled ? (danger ? "rgba(248,113,113,0.12)" : "rgba(255,255,255,0.06)") : "transparent", color: hovered && !disabled ? (danger ? "#f87171" : "rgba(255,255,255,0.92)") : disabled ? "rgba(255,255,255,0.28)" : "rgba(255,255,255,0.72)", cursor: disabled ? "not-allowed" : "pointer", transition: "all 0.12s ease", padding: 0 }}>
      {icon}
      {shortcut && <span style={{ fontSize: 9, fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", color: "rgba(255,255,255,0.28)" }}>{shortcut}</span>}
    </button>
  );
}
