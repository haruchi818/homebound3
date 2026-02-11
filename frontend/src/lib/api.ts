const ENV_BASE = import.meta.env.VITE_API_BASE ?? "";
const API_BASE = ENV_BASE.trim();

function resolveBase() {
  if (API_BASE) return API_BASE;
  if (typeof window !== "undefined") return window.location.origin;
  return "";
}

export function apiUrl(path: string) {
  if (path.startsWith("http")) return path;
  const base = resolveBase();
  return base ? `${base}${path}` : path;
}

export function wsUrl(path: string) {
  const base = resolveBase();
  if (!base) return path;
  const wsBase = base.replace("http://", "ws://").replace("https://", "wss://");
  return `${wsBase}${path}`;
}

export async function fetchJson<T>(
  path: string,
  init?: RequestInit
): Promise<T> {
  const response = await fetch(apiUrl(path), {
    credentials: "include",
    ...init,
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || response.statusText);
  }

  return (await response.json()) as T;
}
