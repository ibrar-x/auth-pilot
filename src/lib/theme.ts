export type AppTheme = "light" | "dark" | "system";

export function resolveTheme(theme: string | null | undefined): "light" | "dark" {
  if (theme === "dark") return "dark";
  if (theme === "light") return "light";

  if (typeof window !== "undefined") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }

  return "light";
}

export function applyTheme(theme: string | null | undefined): void {
  const root = document.documentElement;
  if (resolveTheme(theme) === "dark") {
    root.classList.add("dark");
  } else {
    root.classList.remove("dark");
  }
}
