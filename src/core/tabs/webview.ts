import { invoke, COMMANDS } from "@shared/tauri";
import type { BrowserBounds } from "@shared/types";

export async function ensureBrowserWebview(url: string, bounds: BrowserBounds): Promise<void> {
  await invoke(COMMANDS.ENSURE_BROWSER_WEBVIEW, { url, bounds });
}

export async function setBrowserBounds(bounds: BrowserBounds): Promise<void> {
  await invoke(COMMANDS.SET_BROWSER_BOUNDS, { bounds });
}

export async function navigateBrowser(url: string): Promise<void> {
  await invoke(COMMANDS.NAVIGATE_BROWSER, { url });
}

export async function reloadBrowser(): Promise<void> {
  await invoke(COMMANDS.RELOAD_BROWSER);
}

export async function openBrowserDevtools(): Promise<void> {
  await invoke(COMMANDS.OPEN_BROWSER_DEVTOOLS);
}

export async function scrollBrowserTo(x: number, y: number): Promise<void> {
  await invoke(COMMANDS.SCROLL_BROWSER_TO, { x, y });
}
