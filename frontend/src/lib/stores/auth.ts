import { browser } from "$app/environment";
import { writable } from "svelte/store";

const API_BASE = import.meta.env.VITE_API_BASE ?? "http://127.0.0.1:8080";

export type UserProfile = {
  id: string;
  displayName: string;
  avatarUrl?: string;
};

export const user = writable<UserProfile | null>(null);

export function startGoogleLogin() {
  if (!browser) return;
  window.location.href = `${API_BASE}/api/auth/google/login`;
}

export async function refreshSession() {
  if (!browser) return null;

  const response = await fetch(`${API_BASE}/api/me`, {
    credentials: "include",
  });

  if (!response.ok) {
    user.set(null);
    return null;
  }

  const data = (await response.json()) as { user: UserProfile };
  user.set(data.user);
  return data.user;
}

export async function signOut() {
  await fetch(`${API_BASE}/api/logout`, {
    method: "POST",
    credentials: "include",
  });
  user.set(null);
}
