import { invoke } from "../../shared/tauri";
import { BrowserBounds } from "@shared/types";

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

export async function scrollBrowserTo(x: number, y: number): Promise<void> {
  await invoke("scroll_browser_to", { x, y });
}
