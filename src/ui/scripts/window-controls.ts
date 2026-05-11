import { getCurrentWindow } from "@shared/tauri";
import { getElement } from "@ui/scripts/utils";

export function initWindowControls(): void {
  const appWindow = getCurrentWindow();

  getElement<HTMLButtonElement>("#minimize-button").addEventListener("click", () => {
    void appWindow.minimize();
  });
  getElement<HTMLButtonElement>("#maximize-button").addEventListener("click", () => {
    void appWindow.toggleMaximize();
  });
  getElement<HTMLButtonElement>("#close-button").addEventListener("click", () => {
    void appWindow.close();
  });
}
