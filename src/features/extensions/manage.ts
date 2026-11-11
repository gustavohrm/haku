import { type ExtensionSource } from "@bindings";
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
const listeners = new Set<() => void>();

function setBusy(next: string | null) {
  busy = next;
  for (const listener of listeners) {
    listener();
  }
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
 * Asks for an extension and adds it, or updates the installed one it is a
 * version of, and says which happened. A failure is reported by the command.
 *
 * @param source - Whether to pick a `.crx` or `.zip` file, or a folder.
 */
export async function addExtension(source: ExtensionSource): Promise<void> {
  if (busy !== null) {
    return;
  }
  setBusy(ADDING);
  try {
    const result = await commands.addExtension(source);
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
