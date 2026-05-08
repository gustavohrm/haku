import { invoke } from "../../shared/tauri";

export interface BrowserBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export async function ensureBrowserWebview(url: string, bounds: BrowserBounds): Promise<void> {
  await invoke("ensure_browser_webview", { url, bounds });
}

export async function setBrowserBounds(bounds: BrowserBounds): Promise<void> {
  await invoke("set_browser_bounds", { bounds });
}

export async function navigateBrowser(url: string): Promise<void> {
  await invoke("navigate_browser", { url });
}

export async function reloadBrowser(): Promise<void> {
  await invoke("reload_browser");
}

export async function openBrowserDevtools(): Promise<void> {
  await invoke("open_browser_devtools");
}
