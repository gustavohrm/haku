import { type ExtensionToAdd } from "@bindings";
import { t } from "@shared/i18n";
import { type KeyboardEvent, useEffect, useRef } from "react";

import { describeAccess } from "./access";
import { answerExtension } from "./manage";

/**
 * The question asked before an extension is installed: what it will be able
 * to do, with what an update adds marked, then Cancel and Add or Update.
 * Focus moves to Cancel, and Esc answers no.
 * [Extensions § Before installing](../../../docs/specs/extensions.md#before-installing).
 */
export function AddQuestion({ asked }: { asked: ExtensionToAdd }) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => cancelRef.current?.focus(), []);
  const onEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      void answerExtension(false);
    }
  };
  const isUpdate = asked.replaces !== null;
  const named = asked.version === "" ? asked.name : `${asked.name} ${asked.version}`;
  const title = t(isUpdate ? "extensions.add.titleUpdate" : "extensions.add.title").replace("{extension}", () => named);
  return (
    <div role="alert" className="flex flex-col gap-2 pb-2 text-sm">
      <p className="font-medium">{title}</p>
      {asked.warnings.length === 0 ? (
        <p className="text-text-secondary">{t("extensions.add.noAccess")}</p>
      ) : (
        <div className="flex flex-col gap-1">
          <p>{t("extensions.add.access")}</p>
          <ul className="flex list-disc flex-col gap-1 ps-5">
            {asked.warnings.map((warning) => (
              <li key={warning.access.kind}>
                {describeAccess(warning.access)}
                {warning.new && <span className="text-text-secondary"> · {t("extensions.add.new")}</span>}
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="flex justify-end gap-2">
        <button
          ref={cancelRef}
          type="button"
          className="btn ghost"
          onClick={() => void answerExtension(false)}
          onKeyDown={onEscape}
        >
          {t("dialog.cancel")}
        </button>
        <button type="button" className="btn primary" onClick={() => void answerExtension(true)} onKeyDown={onEscape}>
          {t(isUpdate ? "extensions.add.confirmUpdate" : "extensions.add.confirm")}
        </button>
      </div>
    </div>
  );
}
