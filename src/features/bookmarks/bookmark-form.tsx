import type { BookmarkId, Bookmarks } from "@bindings";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";
import { useId, useState, type FormEvent, type KeyboardEvent } from "react";

import { addressOf, displayTitle, isRoot, locate, urlOf } from "./tree";

/** What the form is for: a new link or folder at the end of `parent`, or an existing bookmark. */
export type FormTarget = { kind: "addLink" | "addFolder"; parent: BookmarkId } | { kind: "edit"; id: BookmarkId };

interface BookmarkFormProps {
  tree: Bookmarks;
  target: FormTarget;
  /** Called once the change is made, or the form is dismissed. */
  onDone: () => void;
}

/**
 * The inline form that adds a link or folder, or edits one in place: a name,
 * and an address for a link. Enter saves and Escape dismisses. A refused
 * change keeps the form open with what was typed, beside the toast that
 * says why.
 */
export function BookmarkForm({ tree, target, onDone }: BookmarkFormProps) {
  const editing = target.kind === "edit" ? locate(tree, target.id)?.node : undefined;
  const isLink = target.kind === "addLink" || (editing !== undefined && urlOf(editing) !== null);
  const [title, setTitle] = useState(() =>
    editing === undefined || isRoot(tree, editing.id) ? "" : displayTitle(tree, editing),
  );
  const [address, setAddress] = useState(() => (editing === undefined ? "" : (urlOf(editing) ?? "")));
  const [saving, setSaving] = useState(false);
  const id = useId();

  const dismissOnEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      onDone();
    }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const name = title.trim();
    const url = addressOf(address);
    setSaving(true);
    const result =
      target.kind === "edit"
        ? await commands.updateBookmark(target.id, name, isLink ? url : null)
        : target.kind === "addLink"
          ? await commands.addBookmark(target.parent, null, name, url)
          : await commands.addBookmarkFolder(target.parent, null, name || t("bookmarks.newFolder"));
    setSaving(false);
    if (result.status === "ok") {
      onDone();
    }
  };

  return (
    <form
      className="bg-surface flex flex-wrap items-end gap-2 rounded-(--radius-control) p-3"
      onSubmit={(event) => void submit(event)}
    >
      <label className="flex min-w-40 flex-1 flex-col gap-1" htmlFor={`${id}-name`}>
        <span className="text-text-secondary text-sm">{t("bookmarks.name")}</span>
        <input
          id={`${id}-name`}
          className="ipt edged"
          value={title}
          placeholder={isLink ? undefined : t("bookmarks.newFolder")}
          spellCheck={false}
          // oxlint-disable-next-line jsx-a11y/no-autofocus -- The form opens because the user asked to type into it.
          autoFocus
          onChange={(event) => setTitle(event.target.value)}
          onKeyDown={dismissOnEscape}
        />
      </label>
      {isLink && (
        <label className="flex min-w-60 flex-2 flex-col gap-1" htmlFor={`${id}-address`}>
          <span className="text-text-secondary text-sm">{t("bookmarks.address")}</span>
          <input
            id={`${id}-address`}
            className="ipt edged"
            value={address}
            spellCheck={false}
            inputMode="url"
            onChange={(event) => setAddress(event.target.value)}
            onKeyDown={dismissOnEscape}
          />
        </label>
      )}
      <div className="flex gap-2">
        <button type="button" className="btn ghost edged" onClick={onDone}>
          {t("dialog.cancel")}
        </button>
        <button type="submit" className="btn" disabled={saving || (isLink && address.trim() === "")}>
          {t("bookmarks.save")}
        </button>
      </div>
    </form>
  );
}
