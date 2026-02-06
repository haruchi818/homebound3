import { browser } from "$app/environment";
import { writable } from "svelte/store";

export type Theme = "light" | "dark";

function getInitialTheme(): Theme {
  if (!browser) return "light";

  const stored = localStorage.getItem("hb3.theme");
  if (stored === "light" || stored === "dark") {
    return stored;
  }

  const prefersDark = window.matchMedia?.("(prefers-color-scheme: dark)").matches;
  return prefersDark ? "dark" : "light";
}

export const theme = writable<Theme>(getInitialTheme());

theme.subscribe((value) => {
  if (!browser) return;
  localStorage.setItem("hb3.theme", value);
});

export function toggleTheme() {
  theme.update((value) => (value === "dark" ? "light" : "dark"));
}
