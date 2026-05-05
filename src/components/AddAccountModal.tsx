import { useState } from "react";
import {
  describeFileSource,
  isTauriRuntime,
  openExternalUrl,
  pickAuthJsonFile,
  type FileSource,
} from "../lib/platform";

interface AddAccountModalProps {
  isOpen: boolean;
  onClose: () => void;
  onImportFile: (source: FileSource, name: string) => Promise<void>;
  onStartOAuth: (name: string) => Promise<{ auth_url: string }>;
  onCompleteOAuth: () => Promise<unknown>;
  onCancelOAuth: () => Promise<void>;
}

type Tab = "oauth" | "import";

export function AddAccountModal({
  isOpen,
  onClose,
  onImportFile,
  onStartOAuth,
  onCompleteOAuth,
  onCancelOAuth,
}: AddAccountModalProps) {
  const [activeTab, setActiveTab] = useState<Tab>("oauth");
  const [name, setName] = useState("");
  const [fileSource, setFileSource] = useState<FileSource | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [oauthPending, setOauthPending] = useState(false);
  const [authUrl, setAuthUrl] = useState<string>("");
  const [copied, setCopied] = useState<boolean>(false);
  const isPrimaryDisabled = loading || (activeTab === "oauth" && oauthPending);
  void isTauriRuntime(); // ensure runtime check is called

  const resetForm = () => {
    setName("");
    setFileSource(null);
    setError(null);
    setLoading(false);
    setOauthPending(false);
    setAuthUrl("");
  };

  const handleClose = () => {
    if (oauthPending) {
      onCancelOAuth();
    }
    resetForm();
    onClose();
  };

  const handleOAuthLogin = async () => {
    if (!name.trim()) {
      setError("Please enter an account name");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      const info = await onStartOAuth(name.trim());
      setAuthUrl(info.auth_url);
      setOauthPending(true);
      setLoading(false);

      await onCompleteOAuth();
      handleClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setLoading(false);
      setOauthPending(false);
    }
  };

  const handleSelectFile = async () => {
    try {
      const selected = await pickAuthJsonFile();
      if (selected) setFileSource(selected);
    } catch (err) {
      console.error("Failed to open file dialog:", err);
    }
  };

  const handleImportFile = async () => {
    if (!name.trim()) {
      setError("Please enter an account name");
      return;
    }
    if (!fileSource) {
      setError("Please select an auth.json file");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      await onImportFile(fileSource, name.trim());
      handleClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setLoading(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-[#1f1f1f] w-full max-w-md mx-4 rounded-[4px] shadow-l2 overflow-hidden animate-fade-in-up">
        <div className="flex items-center justify-between p-6 pb-4">
          <h2 className="text-lg font-medium text-[#141413] dark:text-[#f3f0ee]">Add Account</h2>
          <button
            onClick={handleClose}
            className="h-8 w-8 flex items-center justify-center rounded-[4px] text-[#696969] hover:text-[#141413] dark:hover:text-[#f3f0ee] hover:bg-[#F3F0EE] dark:hover:bg-[#2a2a2a] transition-colors"
          >
            ✕
          </button>
        </div>

        <div className="flex mx-6 mb-4 bg-[#F3F0EE] dark:bg-[#2a2a2a] rounded-[4px] p-1">
          {(["oauth", "import"] as Tab[]).map((tab) => (
            <button
              key={tab}
              onClick={() => {
                if (tab === "import" && oauthPending) {
                  void onCancelOAuth().catch((err) => {
                    console.error("Failed to cancel login:", err);
                  });
                  setOauthPending(false);
                  setLoading(false);
                }
                setActiveTab(tab);
                setError(null);
              }}
              className={`flex-1 px-4 py-2 text-sm font-medium rounded-[4px] transition-colors ${
                activeTab === tab
                  ? "bg-[#141413] dark:bg-[#f3f0ee] text-[#F3F0EE] dark:text-[#141413]"
                  : "text-[#696969] dark:text-[#9a9a9a] hover:text-[#141413] dark:hover:text-[#f3f0ee]"
              }`}
            >
              {tab === "oauth" ? "ChatGPT Login" : "Import File"}
            </button>
          ))}
        </div>

        <div className="px-6 py-4 space-y-4">
          <div>
            <label className="block text-sm font-medium text-[#141413] dark:text-[#f3f0ee] mb-2">
              Account Name
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g., Work Account"
              className="w-full px-5 py-2.5 bg-white dark:bg-[#1f1f1f] border border-[#141413]/20 dark:border-[#f3f0ee]/20 rounded-[4px] text-[#141413] dark:text-[#f3f0ee] placeholder-[#D1CDC7] dark:placeholder-[#696969] focus:outline-none focus:border-[#141413] dark:focus:border-[#f3f0ee] transition-colors"
            />
          </div>

          {activeTab === "oauth" && (
            <div className="text-sm text-[#696969] dark:text-[#9a9a9a]">
              {oauthPending ? (
                <div className="text-center py-4">
                  <div className="animate-pulse h-8 w-8 bg-[#D1CDC7] dark:bg-[#3a3a3a] rounded-[4px] mx-auto mb-3"></div>
                  <p className="text-[#141413] dark:text-[#f3f0ee] font-medium mb-2">Waiting for browser login...</p>
                  <p className="text-xs text-[#696969] dark:text-[#9a9a9a] mb-4">
                    Please open the following link in your browser:
                  </p>
                  <div className="flex items-center gap-2 mb-2 bg-[#F3F0EE] dark:bg-[#2a2a2a] p-2 rounded-[4px]">
                    <input
                      type="text"
                      readOnly
                      value={authUrl}
                      className="flex-1 bg-transparent border-none text-xs text-[#141413] dark:text-[#f3f0ee] focus:outline-none focus:ring-0 truncate"
                    />
                    <button
                      onClick={() => {
                        void navigator.clipboard
                          .writeText(authUrl)
                          .then(() => {
                            setCopied(true);
                            setTimeout(() => setCopied(false), 2000);
                          })
                          .catch(() => {
                            setError("Clipboard unavailable. Copy manually.");
                          });
                      }}
                      className={`px-3 py-1.5 border rounded-[4px] text-xs font-medium transition-colors shrink-0 ${
                        copied
                          ? "bg-[#F3F0EE] dark:bg-[#2a2a2a] border-[#10b981] text-[#10b981]"
                          : "bg-white dark:bg-[#1f1f1f] border-[#141413]/20 dark:border-[#f3f0ee]/20 text-[#141413] dark:text-[#f3f0ee] hover:bg-[#F3F0EE] dark:hover:bg-[#2a2a2a]"
                      }`}
                    >
                      {copied ? "Copied!" : "Copy"}
                    </button>
                    <button
                      onClick={() => {
                        void openExternalUrl(authUrl);
                      }}
                      className="px-3 py-1.5 bg-[#141413] hover:bg-[#262627] dark:bg-[#f3f0ee] dark:hover:bg-white border border-[#141413] dark:border-[#f3f0ee] rounded-[4px] text-xs font-medium text-[#F3F0EE] dark:text-[#141413] transition-colors shrink-0"
                    >
                      Open
                    </button>
                  </div>
                </div>
              ) : (
                <p>
                  Click the button below to generate a login link.
                  You will need to open it in your browser to authenticate.
                </p>
              )}
            </div>
          )}

          {activeTab === "import" && (
            <div>
              <label className="block text-sm font-medium text-[#141413] dark:text-[#f3f0ee] mb-2">
                Select auth.json file
              </label>
              <div className="flex gap-2">
                <div className="flex-1 px-5 py-2.5 bg-[#F3F0EE] dark:bg-[#2a2a2a] border border-[#D1CDC7] dark:border-[#3a3a3a] rounded-[4px] text-sm text-[#141413] dark:text-[#f3f0ee] truncate">
                  {describeFileSource(fileSource)}
                </div>
                <button
                  onClick={handleSelectFile}
                  className="px-5 py-2.5 bg-[#F3F0EE] hover:bg-[#D1CDC7] dark:bg-[#2a2a2a] dark:hover:bg-[#3a3a3a] border border-[#D1CDC7] dark:border-[#3a3a3a] rounded-[4px] text-sm font-medium text-[#141413] dark:text-[#f3f0ee] transition-colors whitespace-nowrap"
                >
                  Browse...
                </button>
              </div>
              <p className="text-xs text-[#D1CDC7] dark:text-[#696969] mt-2">
                Import credentials from an existing Codex auth.json file
              </p>
            </div>
          )}

          {error && (
            <div className="p-4 bg-[#F3F0EE] dark:bg-[#2a2a2a] border border-[#CF4500] rounded-[4px] text-[#CF4500] text-sm">
              {error}
            </div>
          )}
        </div>

        <div className="flex gap-3 p-6 pt-4">
          <button
            onClick={handleClose}
            className="flex-1 px-5 py-2.5 text-sm font-medium rounded-[4px] bg-[#F3F0EE] hover:bg-[#D1CDC7] dark:bg-[#2a2a2a] dark:hover:bg-[#3a3a3a] text-[#141413] dark:text-[#f3f0ee] transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={activeTab === "oauth" ? handleOAuthLogin : handleImportFile}
            disabled={isPrimaryDisabled}
            className="flex-1 px-5 py-2.5 text-sm font-medium rounded-[4px] bg-[#141413] hover:bg-[#262627] dark:bg-[#f3f0ee] dark:hover:bg-white text-[#F3F0EE] dark:text-[#141413] transition-colors disabled:opacity-50"
          >
            {loading
              ? "Adding..."
              : activeTab === "oauth"
                ? "Generate Login Link"
                : "Import"}
          </button>
        </div>
      </div>
    </div>
  );
}
