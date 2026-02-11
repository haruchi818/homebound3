import { browser } from "$app/environment";
import { writable } from "svelte/store";
import { apiUrl } from "$lib/api";

export type UserProfile = {
  id: string;
  displayName: string;
  avatarUrl?: string;
  email?: string;
};

export const user = writable<UserProfile | null>(null);

export function startGoogleLogin() {
  if (!browser) return;
  window.location.href = apiUrl("/api/auth/google/login");
}

export async function refreshSession() {
  if (!browser) return null;

  const response = await fetch(apiUrl("/api/me"), {
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
  await fetch(apiUrl("/api/logout"), {
    method: "POST",
    credentials: "include",
  });
  user.set(null);
}
