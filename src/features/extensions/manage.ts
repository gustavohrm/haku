import { type ExtensionSource, type ExtensionStore, type ExtensionToAdd, type ExtensionUpdate } from "@bindings";
import { feedback, notifyFailure } from "@features/feedback/feedback";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";
import { useSyncExternalStore } from "react";

/**
 * Adding, updating and uninstalling extensions from settings, adding them
 * from a store, and checking the stores for updates, one at a time.
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

/** What is changing while the stores are checked for updates, which can install some. */
export const CHECKING = ".check";

/** What is changing: {@link ADDING}, {@link CHECKING}, or the folder of the extension being removed. */
let busy: string | null = null;
/** Where the question about an extension is asked: in settings, or under the toolbar's **Add to Haku**. */
export type AskedFrom = "settings" | "store";

/** The extension unpacked and waiting for the user to say whether to install it. */
let asking: ExtensionToAdd | null = null;
let askedFrom: AskedFrom = "settings";
/** Whether a store download is running, and whether its button has gone since it started. */
let storeDownloading = false;
let storeLeft = false;
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

function setAsking(next: ExtensionToAdd | null, from: AskedFrom = askedFrom) {
  asking = next;
  askedFrom = from;
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
 * The extension waiting for the user's answer, when it is asked about from
 * `from`. Settings asks on the row of the extension it updates, or under
 * **Add an extension**; the store's under **Add to Haku**.
 *
 * @param from - Where the caller asks.
 */
export function useExtensionToAdd(from: AskedFrom): ExtensionToAdd | null {
  return useSyncExternalStore(subscribe, () => (askedFrom === from ? asking : null));
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
      setAsking(waiting, "settings");
    }
  }
}

/**
 * Downloads an extension from a store and checks it, then waits for
 * {@link answerExtension} while the toolbar shows what it may do. Adding
 * stays busy until then. A failure is reported by the command.
 * [Extensions § From the stores](../../../docs/specs/extensions.md#from-the-stores).
 *
 * @param store - The store the extension is in.
 * @param id - The extension's id in the store.
 */
export async function addFromStore(store: ExtensionStore, id: string): Promise<void> {
  if (busy !== null) {
    return;
  }
  setBusy(ADDING);
  storeDownloading = true;
  storeLeft = false;
  let waiting: ExtensionToAdd | null = null;
  try {
    const result = await commands.addExtensionFromStore(store, id);
    waiting = result.status === "ok" ? result.data : null;
  } finally {
    storeDownloading = false;
    if (waiting !== null && storeLeft) {
      // Nothing is left to ask from, so it is answered no.
      void commands.confirmExtension(waiting.token, false);
      waiting = null;
    }
    if (waiting === null) {
      setBusy(null);
    } else {
      setAsking(waiting, "store");
    }
  }
}

/**
 * Downloads an update that a check of the stores kept for review, then waits for
 * {@link answerExtension} while settings shows what it may do, under the
 * extension's row. A failure is reported by the command.
 * [Extensions § Updates from the stores](../../../docs/specs/extensions.md#updates-from-the-stores).
 *
 * @param update - The update waiting.
 */
export async function reviewUpdate(update: ExtensionUpdate): Promise<void> {
  if (busy !== null) {
    return;
  }
  setBusy(ADDING);
  let waiting: ExtensionToAdd | null = null;
  try {
    const result = await commands.addExtensionFromStore(update.store, update.id);
    waiting = result.status === "ok" ? result.data : null;
  } finally {
    if (waiting === null) {
      setBusy(null);
    } else {
      setAsking(waiting, "settings");
    }
  }
}

/**
 * Checks the stores for updates now, and says what the check did: the
 * extensions it updated, those waiting for review, that all are up to date,
 * or what failed.
 * [Extensions § Updates from the stores](../../../docs/specs/extensions.md#updates-from-the-stores).
 */
export async function checkForUpdates(): Promise<void> {
  if (busy !== null) {
    return;
  }
  setBusy(CHECKING);
  try {
    const result = await commands.checkExtensionUpdates();
    if (result.status === "error") {
      return;
    }
    const { updated, waiting, error } = result.data;
    for (const { extension, replaced } of updated) {
      feedback().success({
        title: t("extensions.updated").replace("{name}", extension.name),
        description: t("extensions.updated.versions")
          .replace("{from}", replaced ?? "")
          .replace("{to}", extension.version),
      });
    }
    if (waiting.length > 0) {
      feedback().info({
        title:
          waiting.length === 1
            ? t("extensions.updates.waiting.one")
            : t("extensions.updates.waiting.many").replace("{count}", String(waiting.length)),
        description: t("extensions.updates.waiting.help"),
      });
    }
    if (error !== null) {
      notifyFailure(error);
    } else if (updated.length === 0 && waiting.length === 0) {
      feedback().success({ title: t("extensions.updates.none") });
    }
  } finally {
    setBusy(null);
  }
}

/**
 * Called when **Add to Haku** goes, as when the tab leaves the store page:
 * the question it asks is answered no, and so is one still downloading, once
 * it arrives.
 */
export function leaveStore(): void {
  if (asking !== null && askedFrom === "store") {
    void answerExtension(false);
  } else if (storeDownloading) {
    storeLeft = true;
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
