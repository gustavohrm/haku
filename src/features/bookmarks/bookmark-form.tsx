import type { BookmarkId, Bookmarks } from "@bindings";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useLayoutEffect, useState, type FormEvent, type KeyboardEvent, type RefObject } from "react";

import { Favicon } from "./favicon";
import { addressOf, displayTitle, isRoot, locate, urlOf } from "./tree";

/** What the form is for: a new link or folder at the end of `parent`, or an existing bookmark. */
export type FormTarget = { kind: "addLink" | "addFolder"; parent: BookmarkId } | { kind: "edit"; id: BookmarkId };

interface BookmarkFormProps {
  tree: Bookmarks;
  target: FormTarget;
  /**
   * `popover`: the fields stacked, **Cancel** on the left and **Save** on the
   * right. `row`: one line in place of a list row, the fields and **Save**.
   */
  layout: "popover" | "row";
  /**
   * Set to how the form is dismissed, for a popover closed by a click away:
   * an edit is kept, as the star's editor keeps it, and an addition is
   * discarded. Escape in a field dismisses the same way.
   */
  dismissRef?: RefObject<(() => void) | null>;
  /** Called once the change is made, or the form is dismissed. */
  onDone: () => void;
}

/**
 * The form that adds a link or folder, or edits one: a name, led by the
 * link's icon or a folder's, and an address for a link, led by a link icon.
 * Neither field has a visible label: the icon and the value say what each is,
 * and an empty one says so in its placeholder. Enter saves.
 * A refused change keeps the form open with what was typed, beside the toast
 * that says why.
 *
 * Escape discards, except for an edit in a popover, which keeps what was
 * typed, as [Interface guidelines § Popovers and panels] has it.
 *
 * [Interface guidelines § Popovers and panels]: ../../../docs/guidelines/interface.md#popovers-and-panels
 */
export function BookmarkForm({ tree, target, layout, dismissRef, onDone }: BookmarkFormProps) {
  const { private: isPrivate } = useBrowserState();
  const editing = target.kind === "edit" ? locate(tree, target.id)?.node : undefined;
  const isLink = target.kind === "addLink" || (editing !== undefined && urlOf(editing) !== null);
  const [initial] = useState(() => ({
    title: editing === undefined || isRoot(tree, editing.id) ? "" : displayTitle(tree, editing),
    address: editing === undefined ? "" : (urlOf(editing) ?? ""),
  }));
  const [title, setTitle] = useState(initial.title);
  const [address, setAddress] = useState(initial.address);
  const [saving, setSaving] = useState(false);
  const missingAddress = isLink && address.trim() === "";

  const dismiss = async () => {
    const changed = title !== initial.title || address !== initial.address;
    // An empty address is not saved, as Save is disabled without one.
    if (layout === "popover" && target.kind === "edit" && changed && !missingAddress) {
      if (saving) {
        return;
      }
      setSaving(true);
      const result = await commands.updateBookmark(target.id, title.trim(), isLink ? addressOf(address) : null);
      setSaving(false);
      // Refused, as an address Rust will not keep is: the form stays, as it does when Save is refused.
      if (result.status !== "ok") {
        return;
      }
    }
    onDone();
  };
  useLayoutEffect(() => {
    if (dismissRef) {
      dismissRef.current = () => void dismiss();
    }
  });

  const dismissOnEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      void dismiss();
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

  // The icon of the address the bookmark has, not of what is being typed,
  // which would fetch an icon for every keystroke.
  const icon = isLink ? (
    <Favicon key={initial.address} url={isPrivate || initial.address === "" ? null : initial.address} small />
  ) : (
    <i className="ic-folder ic-sm text-text-secondary shrink-0" aria-hidden="true" />
  );
  const isRow = layout === "row";
  const save = (
    <button type="submit" className={cx("btn", !isRow && "ml-auto")} disabled={saving || missingAddress}>
      {t("bookmarks.save")}
    </button>
  );

  return (
    <form
      className={isRow ? "flex min-h-10 items-center gap-2 py-1" : "flex flex-col gap-1.5"}
      onSubmit={(event) => void submit(event)}
    >
      <span className={cx("input-group soft edgeless unadorned", isRow && "min-w-0 flex-1")}>
        {icon}
        <input
          className="ipt"
          aria-label={t("bookmarks.name")}
          value={title}
          placeholder={isLink ? t("bookmarks.name") : t("bookmarks.newFolder")}
          spellCheck={false}
          autoComplete="off"
          // oxlint-disable-next-line jsx-a11y/no-autofocus -- The form opens because the user asked to type into it.
          autoFocus
          onChange={(event) => setTitle(event.target.value)}
          onKeyDown={dismissOnEscape}
        />
      </span>
      {isLink && (
        <span className={cx("input-group soft edgeless unadorned", isRow && "min-w-0 flex-2")}>
          <i className="ic-link ic-sm text-text-secondary shrink-0" aria-hidden="true" />
          <input
            className="ipt"
            aria-label={t("bookmarks.address")}
            placeholder={t("bookmarks.address")}
            value={address}
            spellCheck={false}
            autoComplete="off"
            inputMode="url"
            onChange={(event) => setAddress(event.target.value)}
            onKeyDown={dismissOnEscape}
          />
        </span>
      )}
      {isRow ? (
        save
      ) : (
        <div className="mt-1.5 flex items-center gap-2">
          <button type="button" className="btn ghost" onClick={onDone}>
            {t("dialog.cancel")}
          </button>
          {save}
        </div>
      )}
    </form>
  );
}
