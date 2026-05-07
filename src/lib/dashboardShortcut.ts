import { invokeBackend, isTauriRuntime } from "./platform";
import type { AppSettings } from "../types";

const DEFAULT_DASHBOARD_SHORTCUT = "CommandOrControl+Shift+A";

export function getDashboardShortcut(settings: AppSettings | null | undefined): string {
  if (!settings || settings.dashboard_global_shortcut === undefined) {
    return DEFAULT_DASHBOARD_SHORTCUT;
  }

  return settings.dashboard_global_shortcut.trim();
}

export async function registerDashboardShortcut(shortcut: string): Promise<void> {
  if (!isTauriRuntime()) return;

  const { register, unregisterAll } = await import("@tauri-apps/plugin-global-shortcut");
  await unregisterAll();
  if (!shortcut.trim()) return;
  await register(shortcut.trim(), () => {
    invokeBackend("show_main_window").catch((err) => {
      console.error("Failed to open dashboard from global shortcut:", err);
    });
  });
}
