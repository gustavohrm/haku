import { type ExtensionSource, type ExtensionToAdd } from "@bindings";
import { feedback } from "@features/feedback/feedback";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";
import { useSyncExternalStore } from "react";

/**
 * Adding, updating and uninstalling extensions from settings, one at a time.
 *
 * Rust serializes the changes anyway; this keeps their buttons disabled while
 * one runs, wherever they are, so a second click does not queue a second
 * picker or a second uninstall behind the first.
 * [Extensions § Settings](../../../docs/specs/extensions.md#settings).
 */

/**
 * What is changing while an add runs. A folder starting with `.` is never an
 * extension's, so this is never the folder of one being removed.
 */
export const ADDING = ".add";

/** What is changing: {@link ADDING}, or the folder of the extension being removed. */
let busy: string | null = null;
/** The extension unpacked and waiting for the user to say whether to install it. */
let asking: ExtensionToAdd | null = null;
const listeners = new Set<() => void>();

function notify() {
  for (const listener of listeners) {
    listener();
  }
}

function setBusy(next: string | null) {
  busy = next;
  notify();
}

function setAsking(next: ExtensionToAdd | null) {
  asking = next;
  notify();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** What is changing now, or `null` while nothing is. */
export function useExtensionChange(): string | null {
  return useSyncExternalStore(subscribe, () => busy);
}

/**
 * The extension waiting for the user's answer, which settings asks about on
 * the row of the extension it updates, or under **Add an extension**.
 */
export function useExtensionToAdd(): ExtensionToAdd | null {
  return useSyncExternalStore(subscribe, () => asking);
}

/**
 * Asks for an extension and unpacks it, then waits for {@link answerExtension}
 * while settings shows what it may do. Adding stays busy until then. A
 * failure is reported by the command.
 *
 * @param source - Whether to pick a `.crx` or `.zip` file, or a folder.
 */
export async function addExtension(source: ExtensionSource): Promise<void> {
  if (busy !== null) {
    return;
  }
  setBusy(ADDING);
  let waiting: ExtensionToAdd | null = null;
  try {
    const result = await commands.addExtension(source);
    waiting = result.status === "ok" ? result.data : null;
  } finally {
    if (waiting === null) {
      setBusy(null);
    } else {
      setAsking(waiting);
    }
  }
}

/**
 * Answers the question about the extension waiting: installs it, adding it or
 * updating the one it is a version of, and says which happened, or deletes
 * it. A failure is reported by the command.
 *
 * @param install - Whether to install it.
 */
export async function answerExtension(install: boolean): Promise<void> {
  const waiting = asking;
  if (waiting === null) {
    return;
  }
  setAsking(null);
  try {
    const result = await commands.confirmExtension(waiting.token, install);
    if (result.status === "error" || result.data === null) {
      return;
    }
    const { extension, replaced } = result.data;
    if (replaced === null) {
      feedback().success({ title: t("extensions.added").replace("{name}", extension.name) });
    } else {
      feedback().success({
        title: t("extensions.updated").replace("{name}", extension.name),
        description:
          replaced === extension.version
            ? t("extensions.updated.same").replace("{version}", extension.version)
            : t("extensions.updated.versions").replace("{from}", replaced).replace("{to}", extension.version),
      });
    }
  } finally {
    setBusy(null);
  }
}

/**
 * Uninstalls an extension with what it stored, or deletes a folder that
 * failed to install. Settings asks on the extension's row first. A failure is
 * reported by the command.
 *
 * @param folder - The extension's folder.
 */
export async function uninstallExtension(folder: string): Promise<void> {
  if (busy !== null) {
    return;
  }
  setBusy(folder);
  try {
    await commands.removeExtension(folder);
  } finally {
    setBusy(null);
  }
}
