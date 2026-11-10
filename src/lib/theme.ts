import { useSyncExternalStore } from "react";

/** User choice; "system" follows macOS (and changes with it). */
export type Theme = "light" | "dark" | "system";

const KEY = "lezioni:theme";
const media = window.matchMedia("(prefers-color-scheme: dark)");

function read(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

let theme: Theme = read();
const listeners = new Set<() => void>();

const resolve = (t: Theme): "light" | "dark" => (t === "system" ? (media.matches ? "dark" : "light") : t);

/** `.dark` on <html> drives Tailwind's dark: variant (see index.css). */
function apply() {
  const dark = resolve(theme) === "dark";
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
  // Native title bar / window chrome. Outside Tauri (browser previews) there is no window.
  import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(theme === "system" ? null : theme))
    .catch(() => {});
  listeners.forEach((l) => l());
}

media.addEventListener("change", () => theme === "system" && apply());
apply();

export function setTheme(t: Theme) {
  theme = t;
  try {
    if (t === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, t);
  } catch {
    /* not persisted */
  }
  apply();
}

let snapshot = { theme, resolved: resolve(theme) };
export function useTheme(): { theme: Theme; resolved: "light" | "dark" } {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => {
      const resolved = resolve(theme);
      if (snapshot.theme !== theme || snapshot.resolved !== resolved) snapshot = { theme, resolved };
      return snapshot;
    },
  );
}
