import { invoke } from "@tauri-apps/api/core";
export type Item = {
  id: string;
  revision: number;
  heading: string;
  text: string;
  source: string;
  updated: number;
  kind: string;
  pinned: boolean;
  color?: string;
};
export type Preferences = {
  enabled: boolean;
  paused: boolean;
  clipboard: boolean;
  autostart: boolean;
  retention_days: number;
  allowed_apps: string[];
  browser_capture: boolean;
};
export type Status = {
  prefs: Preferences;
  last_saved: number;
  error?: string;
  data_dir?: string;
  capture?: string;
};
export async function api<T>(payload: Record<string, unknown>): Promise<T> {
  return invoke<T>("request", { payload });
}
export async function openFolder() {
  return invoke("open_data_folder");
}
export async function setupBrowser() {
  return invoke<string>("setup_browser");
}
