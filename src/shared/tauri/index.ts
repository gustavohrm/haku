import { invoke as invokeTauri } from "@tauri-apps/api/core";
import { listen as listenTauri, Event } from "@tauri-apps/api/event";

import { COMMANDS, EVENTS } from "./constants";

type TauriCommand = (typeof COMMANDS)[keyof typeof COMMANDS];
type TauriEvent = (typeof EVENTS)[keyof typeof EVENTS];

async function invoke<T>(cmd: TauriCommand, args?: any): Promise<T> {
  return invokeTauri<T>(cmd, args);
}

async function listen<T>(event: TauriEvent, callback: (event: Event<T>) => void): Promise<() => void> {
  return listenTauri<T>(event, callback);
}

export { invoke, listen, COMMANDS, EVENTS };

export { getCurrentWindow } from "@tauri-apps/api/window";
