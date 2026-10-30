// Fixed chord hints; tauri/src/model/keymap.rs is the source of truth.
export const SHORTCUT_HINTS = {
  newTab: "Ctrl+T",
  reopen: "Ctrl+Shift+T",
  close: "Ctrl+W",
  reload: "Ctrl+R",
  duplicate: "Ctrl+Shift+K",
  separate: "Alt+Shift+N",
  back: "Alt+Left",
  forward: "Alt+Right",
  inspect: "F12",
} as const;
