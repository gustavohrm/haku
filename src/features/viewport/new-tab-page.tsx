import { t } from "@shared/i18n";

export function NewTabPage() {
  return (
    <div className="bg-background grid h-full place-content-center gap-2 text-center select-text">
      <h1 className="text-title">{t("newTab.title")}</h1>
      <p className="text-text-secondary">{t("newTab.prompt")}</p>
    </div>
  );
}
