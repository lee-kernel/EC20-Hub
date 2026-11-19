import type { ThemeMode } from "./types";

let currentMode: ThemeMode = "light";
let mediaBound = false;

function resolveTheme(mode: ThemeMode): "light" | "dark" {
  if (mode === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return mode;
}

export function applyTheme(mode: ThemeMode = "light") {
  currentMode = mode;
  document.documentElement.dataset.theme = resolveTheme(mode);

  if (!mediaBound && window.matchMedia) {
    mediaBound = true;
    window
      .matchMedia("(prefers-color-scheme: dark)")
      .addEventListener("change", () => {
        if (currentMode === "system") {
          document.documentElement.dataset.theme = resolveTheme("system");
        }
      });
  }
}
