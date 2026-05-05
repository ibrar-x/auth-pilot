import { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAccounts, useSettings, useSwitchLog } from "../hooks/useAccounts";
import { AccountCard, AddAccountModal, Settings, SwitchLog } from "./";

const appWindow = getCurrentWindow();
const isMacOs =
  typeof navigator !== "undefined" &&
  /(Mac|iPhone|iPod|iPad)/i.test(navigator.userAgent);

export function Dashboard() {
  const {
    accounts,
    loading,
    error,
    refreshUsage,
    refreshSingleUsage,
    switchAccount,
    deleteAccount,
    renameAccount,
    importFromFile,
    importAccountsSlimText,
    startOAuthLogin,
    completeOAuthLogin,
    cancelOAuthLogin,
    loadMaskedAccountIds,
    saveMaskedAccountIds,
  } = useAccounts();

  const { settings, saveSettings } = useSettings();
  const { events: switchEvents } = useSwitchLog();

  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isSwitchLogOpen, setIsSwitchLogOpen] = useState(false);
  const [isConfigModalOpen, setIsConfigModalOpen] = useState(false);
  const [configModalMode] = useState<"slim_export" | "slim_import">("slim_export");
  const [configPayload, setConfigPayload] = useState("");
  const [configModalError, setConfigModalError] = useState<string | null>(null);
  const [configCopied, setConfigCopied] = useState(false);
  const [switchingId, setSwitchingId] = useState<string | null>(null);
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isExportingSlim] = useState(false);
  const [isImportingSlim, setIsImportingSlim] = useState(false);
  const [refreshSuccess, setRefreshSuccess] = useState(false);
  const [maskedAccounts, setMaskedAccounts] = useState<Set<string>>(new Set());

  useEffect(() => {
    loadMaskedAccountIds().then((ids) => {
      if (ids.length > 0) {
        setMaskedAccounts(new Set(ids));
      }
    });
  }, [loadMaskedAccountIds]);

  const toggleMask = (accountId: string) => {
    setMaskedAccounts((prev) => {
      const next = new Set(prev);
      if (next.has(accountId)) {
        next.delete(accountId);
      } else {
        next.add(accountId);
      }
      void saveMaskedAccountIds(Array.from(next));
      return next;
    });
  };

  const handleSwitch = async (accountId: string) => {
    try {
      setSwitchingId(accountId);
      await switchAccount(accountId);
    } catch (err) {
      console.error("Failed to switch account:", err);
    } finally {
      setSwitchingId(null);
    }
  };

  const handleDelete = async (accountId: string) => {
    if (deleteConfirmId !== accountId) {
      setDeleteConfirmId(accountId);
      setTimeout(() => setDeleteConfirmId(null), 3000);
      return;
    }

    try {
      await deleteAccount(accountId);
      setDeleteConfirmId(null);
    } catch (err) {
      console.error("Failed to delete account:", err);
    }
  };

  const handleRefresh = async () => {
    setIsRefreshing(true);
    setRefreshSuccess(false);
    try {
      await refreshUsage(undefined, { refreshMetadata: true });
      setRefreshSuccess(true);
      setTimeout(() => setRefreshSuccess(false), 2000);
    } finally {
      setIsRefreshing(false);
    }
  };

  const handleImportSlimText = async () => {
    if (!configPayload.trim()) {
      setConfigModalError("Please paste the slim text string first.");
      return;
    }

    try {
      setIsImportingSlim(true);
      setConfigModalError(null);
      await importAccountsSlimText(configPayload);
      setMaskedAccounts(new Set());
      setIsConfigModalOpen(false);
    } catch (err) {
      console.error("Failed to import slim text:", err);
      const message = err instanceof Error ? err.message : String(err);
      setConfigModalError(message);
    } finally {
      setIsImportingSlim(false);
    }
  };

  const activeAccount = accounts.find((a) => a.is_active);
  const otherAccounts = accounts.filter((a) => !a.is_active);

  return (
    <div className="min-h-screen bg-[#F3F0EE] dark:bg-[#1a1a1a] text-[#141413] dark:text-[#f3f0ee]">
      {/* Title bar drag region */}
      <div className="sticky top-0 z-40">
        <div className="flex h-9 items-center px-3">
          <div className="h-full flex-1 select-none cursor-default" />
          {!isMacOs && (
            <div className="flex items-center gap-1">
              <button
                onClick={() => void appWindow.minimize()}
                className="flex h-8 w-8 items-center justify-center rounded-md text-[#696969] hover:bg-[#F3F0EE] dark:hover:bg-[#2a2a2a]"
                title="Minimize"
              >
                <svg className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path d="M5 12h14" strokeWidth="2" strokeLinecap="round" />
                </svg>
              </button>
              <button
                onClick={() => void appWindow.close()}
                className="flex h-8 w-8 items-center justify-center rounded-md text-[#696969] hover:bg-[#CF4500] hover:text-white"
                title="Close"
              >
                <svg className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path d="M6 6l12 12M18 6L6 18" strokeWidth="2" strokeLinecap="round" />
                </svg>
              </button>
            </div>
          )}
        </div>

        {/* Floating Pill Nav */}
        <div className="max-w-5xl mx-auto px-6 pb-4">
          <nav className="flex items-center justify-between gap-4 bg-white dark:bg-[#1f1f1f] rounded-[4px] px-5 py-3 shadow-l1">
            <div className="flex items-center gap-3">
              <div className="h-10 w-10 rounded-[4px] bg-[#141413] dark:bg-[#f3f0ee] flex items-center justify-center text-[#F3F0EE] dark:text-[#141413] font-medium text-lg">
                C
              </div>
              <div>
                <h1 className="text-lg font-medium text-[#141413] dark:text-[#f3f0ee] tracking-tight">
                  AuthPilot
                </h1>
                <p className="text-xs text-[#696969] dark:text-[#9a9a9a]">
                  Intelligent multi-account manager
                </p>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <button
                onClick={handleRefresh}
                disabled={isRefreshing}
                className="flex h-10 w-10 items-center justify-center rounded-[4px] bg-[#F3F0EE] dark:bg-[#2a2a2a] text-[#141413] dark:text-[#f3f0ee] hover:bg-[#D1CDC7] dark:hover:bg-[#3a3a3a] disabled:opacity-50 transition-colors"
                title="Refresh all usage"
              >
                <svg
                  className={isRefreshing ? "animate-spin h-5 w-5" : "h-5 w-5"}
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                  />
                </svg>
              </button>
              <button
                onClick={() => setIsSettingsOpen(true)}
                className="flex h-10 w-10 items-center justify-center rounded-[4px] bg-[#F3F0EE] dark:bg-[#2a2a2a] text-[#141413] dark:text-[#f3f0ee] hover:bg-[#D1CDC7] dark:hover:bg-[#3a3a3a] transition-colors"
                title="Settings"
              >
                <svg className="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <circle cx="12" cy="12" r="3" />
                  <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
                </svg>
              </button>
              <button
                onClick={() => setIsSwitchLogOpen(true)}
                className="flex h-10 w-10 items-center justify-center rounded-[4px] bg-[#F3F0EE] dark:bg-[#2a2a2a] text-[#141413] dark:text-[#f3f0ee] hover:bg-[#D1CDC7] dark:hover:bg-[#3a3a3a] transition-colors"
                title="Switch Log"
              >
                <svg className="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <polyline points="14 2 14 8 20 8" />
                  <line x1="16" y1="13" x2="8" y2="13" />
                  <line x1="16" y1="17" x2="8" y2="17" />
                  <polyline points="10 9 9 9 8 9" />
                </svg>
              </button>
              <button
                onClick={() => setIsAddModalOpen(true)}
                className="h-10 px-6 text-sm font-medium rounded-[4px] bg-[#141413] hover:bg-[#262627] dark:bg-[#f3f0ee] dark:hover:bg-white text-[#F3F0EE] dark:text-[#141413] transition-colors"
              >
                Add Account
              </button>
            </div>
          </nav>
        </div>
      </div>

      {/* Main Content */}
      <main className="max-w-5xl mx-auto px-6 py-8">
        {loading && accounts.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20">
            <div className="animate-pulse h-10 w-10 bg-[#D1CDC7] rounded-[4px] mb-4"></div>
            <p className="text-[#D1CDC7] text-sm">Loading accounts...</p>
          </div>
        ) : error ? (
          <div className="text-center py-20">
            <div className="text-[#CF4500] mb-2 text-sm font-medium">Failed to load accounts</div>
            <p className="text-sm text-[#696969]">{error}</p>
          </div>
        ) : accounts.length === 0 ? (
          <div className="text-center py-20">
            <div className="h-16 w-16 rounded-[4px] bg-white dark:bg-[#1f1f1f] flex items-center justify-center mx-auto mb-4 shadow-l1">
              <span className="text-3xl">👤</span>
            </div>
            <h2 className="text-xl font-medium text-[#141413] dark:text-[#f3f0ee] mb-2 tracking-tight">
              No accounts yet
            </h2>
            <p className="text-[#D1CDC7] mb-6 text-sm">
              Add your first account to get started
            </p>
            <button
              onClick={() => setIsAddModalOpen(true)}
              className="px-6 py-3 text-sm font-medium rounded-[4px] bg-[#141413] hover:bg-[#262627] dark:bg-[#f3f0ee] dark:hover:bg-white text-[#F3F0EE] dark:text-[#141413] transition-colors"
            >
              Add Account
            </button>
          </div>
        ) : (
          <div className="space-y-12">
            {/* Active Account */}
            {activeAccount && (
              <section>
                <div className="eyebrow text-[#696969] dark:text-[#9a9a9a] mb-4">
                  Active Account
                </div>
                <AccountCard
                  account={activeAccount}
                  onSwitch={() => {}}
                  onDelete={() => handleDelete(activeAccount.id)}
                  onRefresh={() => refreshSingleUsage(activeAccount.id, { refreshMetadata: true })}
                  onRename={(newName) => renameAccount(activeAccount.id, newName)}
                  switching={switchingId === activeAccount.id}
                  masked={maskedAccounts.has(activeAccount.id)}
                  onToggleMask={() => toggleMask(activeAccount.id)}
                />
              </section>
            )}

            {/* Other Accounts */}
            {otherAccounts.length > 0 && (
              <section>
                <div className="eyebrow text-[#696969] dark:text-[#9a9a9a] mb-4">
                  Other Accounts ({otherAccounts.length})
                </div>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
                  {otherAccounts.map((account) => (
                    <AccountCard
                      key={account.id}
                      account={account}
                      onSwitch={() => handleSwitch(account.id)}
                      onDelete={() => handleDelete(account.id)}
                      onRefresh={() => refreshSingleUsage(account.id, { refreshMetadata: true })}
                      onRename={(newName) => renameAccount(account.id, newName)}
                      switching={switchingId === account.id}
                      masked={maskedAccounts.has(account.id)}
                      onToggleMask={() => toggleMask(account.id)}
                    />
                  ))}
                </div>
              </section>
            )}
          </div>
        )}
      </main>

      {/* Refresh Success Toast */}
      {refreshSuccess && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 px-5 py-3 bg-[#141413] dark:bg-[#f3f0ee] text-[#F3F0EE] dark:text-[#141413] rounded-[4px] shadow-l2 text-sm flex items-center gap-2 animate-fade-in-up">
          <span className="text-[#F37338]">✓</span> Usage refreshed successfully
        </div>
      )}

      {/* Delete Confirmation Toast */}
      {deleteConfirmId && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 px-5 py-3 bg-[#CF4500] text-white rounded-[4px] shadow-l2 text-sm animate-fade-in-up">
          Click delete again to confirm removal
        </div>
      )}

      {/* Add Account Modal */}
      <AddAccountModal
        isOpen={isAddModalOpen}
        onClose={() => setIsAddModalOpen(false)}
        onImportFile={importFromFile}
        onStartOAuth={startOAuthLogin}
        onCompleteOAuth={completeOAuthLogin}
        onCancelOAuth={cancelOAuthLogin}
      />

      {/* Settings Modal */}
      {isSettingsOpen && settings && (
        <Settings
          settings={settings}
          onSave={saveSettings}
          onClose={() => setIsSettingsOpen(false)}
          accounts={accounts}
        />
      )}

      {/* Switch Log Modal */}
      {isSwitchLogOpen && (
        <SwitchLog
          events={switchEvents}
          onClose={() => setIsSwitchLogOpen(false)}
        />
      )}

      {/* Import/Export Config Modal */}
      {isConfigModalOpen && (
        <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-[#1f1f1f] w-full max-w-2xl mx-4 rounded-[4px] shadow-l2 overflow-hidden animate-fade-in-up">
            <div className="flex items-center justify-between p-6 pb-4">
              <h2 className="text-lg font-medium text-[#141413] dark:text-[#f3f0ee]">
                {configModalMode === "slim_export" ? "Export Slim Text" : "Import Slim Text"}
              </h2>
              <button
                onClick={() => setIsConfigModalOpen(false)}
                className="h-8 w-8 flex items-center justify-center rounded-[4px] text-[#696969] hover:text-[#141413] dark:hover:text-[#f3f0ee] hover:bg-[#F3F0EE] dark:hover:bg-[#2a2a2a] transition-colors"
              >
                ✕
              </button>
            </div>
            <div className="px-6 py-4 space-y-4">
              {configModalMode === "slim_import" ? (
                <p className="text-sm text-[#CF4500] dark:text-[#F37338] bg-[#F3F0EE] dark:bg-[#2a2a2a] rounded-[4px] px-4 py-3">
                  Existing accounts are kept. Only missing accounts are imported.
                </p>
              ) : (
                <p className="text-sm text-[#696969] dark:text-[#9a9a9a]">
                  This slim string contains account secrets. Keep it private.
                </p>
              )}
              <textarea
                value={configPayload}
                onChange={(e) => setConfigPayload(e.target.value)}
                readOnly={configModalMode === "slim_export"}
                placeholder={
                  configModalMode === "slim_export"
                    ? isExportingSlim
                      ? "Generating..."
                      : "Export string will appear here"
                    : "Paste config string here"
                }
                className="w-full h-48 px-5 py-4 bg-[#F3F0EE] dark:bg-[#2a2a2a] border border-[#D1CDC7] dark:border-[#3a3a3a] rounded-[4px] text-sm text-[#141413] dark:text-[#f3f0ee] placeholder-[#D1CDC7] dark:placeholder-[#696969] focus:outline-none focus:border-[#141413] dark:focus:border-[#f3f0ee] font-mono"
              />
              {configModalError && (
                <div className="p-4 bg-[#F3F0EE] dark:bg-[#2a2a2a] border border-[#CF4500] rounded-[4px] text-[#CF4500] text-sm">
                  {configModalError}
                </div>
              )}
            </div>
            <div className="flex gap-3 p-6 pt-4">
              <button
                onClick={() => setIsConfigModalOpen(false)}
                className="px-5 py-2.5 text-sm font-medium rounded-[4px] bg-[#F3F0EE] hover:bg-[#D1CDC7] dark:bg-[#2a2a2a] dark:hover:bg-[#3a3a3a] text-[#141413] dark:text-[#f3f0ee] transition-colors"
              >
                Close
              </button>
              {configModalMode === "slim_export" ? (
                <button
                  onClick={async () => {
                    if (!configPayload) return;
                    try {
                      await navigator.clipboard.writeText(configPayload);
                      setConfigCopied(true);
                      setTimeout(() => setConfigCopied(false), 1500);
                    } catch {
                      setConfigModalError("Clipboard unavailable. Please copy manually.");
                    }
                  }}
                  disabled={!configPayload || isExportingSlim}
                  className="px-5 py-2.5 text-sm font-medium rounded-[4px] bg-[#141413] hover:bg-[#262627] dark:bg-[#f3f0ee] dark:hover:bg-white text-[#F3F0EE] dark:text-[#141413] transition-colors disabled:opacity-50"
                >
                  {configCopied ? "Copied" : "Copy String"}
                </button>
              ) : (
                <button
                  onClick={handleImportSlimText}
                  disabled={isImportingSlim}
                  className="px-5 py-2.5 text-sm font-medium rounded-[4px] bg-[#141413] hover:bg-[#262627] dark:bg-[#f3f0ee] dark:hover:bg-white text-[#F3F0EE] dark:text-[#141413] transition-colors disabled:opacity-50"
                >
                  {isImportingSlim ? "Importing..." : "Import Missing Accounts"}
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
