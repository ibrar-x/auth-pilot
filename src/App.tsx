import { useState, useEffect } from "react";
import { Dashboard, TrayPopup } from "./components";
import { invokeBackend, isTauriRuntime } from "./lib/platform";
import type { AppSettings } from "./types";
import "./App.css";

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

function App() {
  const [firstRunRequired, setFirstRunRequired] = useState(false);
  const [consentGiven, setConsentGiven] = useState(false);
  const [windowLabel, setWindowLabel] = useState<string>("main");

  useEffect(() => {
    if (isTauriRuntime()) {
      import("@tauri-apps/api/window")
        .then(({ getCurrentWindow }) => {
          setWindowLabel(getCurrentWindow().label);
        })
        .catch(() => setWindowLabel("main"));
    }
  }, []);

  useEffect(() => {
    if (isTauriRuntime()) {
      invokeBackend<boolean>("is_file_auth_mode_required")
        .then((required) => setFirstRunRequired(required))
        .catch(() => setFirstRunRequired(false));
    }
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      applyTheme("system");
      return;
    }
    invokeBackend<AppSettings>("get_settings")
      .then((settings) => {
        applyTheme(settings.theme || "system");
      })
      .catch(() => {
        applyTheme("system");
      });
  }, []);

  useEffect(() => {
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => {
      if (!isTauriRuntime()) {
        applyTheme("system");
        return;
      }
      invokeBackend<AppSettings>("get_settings")
        .then((settings) => {
          applyTheme(settings.theme || "system");
        })
        .catch(() => applyTheme("system"));
    };
    mql.addEventListener("change", handler);
    return () => mql.removeEventListener("change", handler);
  }, []);

  const handleConsent = async () => {
    try {
      await invokeBackend("ensure_file_auth_mode");
      setConsentGiven(true);
    } catch (err) {
      console.error("Failed to enable file auth mode:", err);
    }
  };

  if (windowLabel === "tray-popup") {
    return (
      <div
        style={{
          width: "100vw",
          height: "100vh",
          overflow: "hidden",
          background: "transparent",
        }}
      >
        <TrayPopup />
      </div>
    );
  }

  if (firstRunRequired && !consentGiven) {
    return (
      <div className="min-h-screen bg-[#F3F0EE] dark:bg-[#1a1a1a] flex items-center justify-center p-6">
        <div className="bg-white dark:bg-[#1f1f1f] rounded-[4px] w-full max-w-md p-10 shadow-l2 text-center animate-fade-in-up">
          <div className="h-16 w-16 rounded-[4px] bg-[#F3F0EE] dark:bg-[#2a2a2a] flex items-center justify-center mx-auto mb-8">
            <span className="text-3xl">🔐</span>
          </div>
          <h2 className="text-2xl font-medium text-[#141413] dark:text-[#f3f0ee] mb-3 tracking-tight">
            First-Time Setup
          </h2>
          <p className="text-[#696969] dark:text-[#9a9a9a] mb-6 text-sm leading-relaxed">
            To enable automatic account switching, AuthPilot needs to store
            credentials in a file instead of the macOS Keychain. This is required
            for the switcher to read and swap accounts.
          </p>
          <div className="bg-[#F3F0EE] dark:bg-[#2a2a2a] border border-[#D1CDC7] dark:border-[#3a3a3a] rounded-[4px] p-4 mb-8">
            <p className="text-xs text-[#CF4500] dark:text-[#F37338]">
              <strong>Warning:</strong> Credentials will be stored in a plaintext
              file at <code className="font-mono text-[11px]">~/.codex/auth.json</code>.
            </p>
          </div>
          <button
            onClick={handleConsent}
            className="w-full px-6 py-3 text-sm font-medium rounded-[4px] bg-[#141413] hover:bg-[#262627] dark:bg-[#f3f0ee] dark:hover:bg-white text-[#F3F0EE] dark:text-[#141413] transition-colors"
          >
            I Understand — Enable File Mode
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-[#F3F0EE] dark:bg-[#1a1a1a] text-[#141413] dark:text-[#f3f0ee]">
      <Dashboard />
    </div>
  );
}

export default App;
