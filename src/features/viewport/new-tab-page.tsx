import { useBrowserState } from "@ipc/hooks";
import { t } from "@shared/i18n";

export function NewTabPage() {
  const { private: isPrivate } = useBrowserState();
  return (
    <div className="bg-background grid h-full place-content-center gap-2 text-center select-text">
      <h1 className="text-title">{t("newTab.title")}</h1>
      <p className="text-text-secondary">{t("newTab.prompt")}</p>
      {isPrivate && (
        <section aria-label={t("private.label")} className="text-text-secondary mx-auto mt-4 max-w-prose px-6">
          <h2 className="text-text mb-2 font-medium">{t("private.label")}</h2>
          <p>{t("private.notice")}</p>
        </section>
      )}
    </div>
  );
}
