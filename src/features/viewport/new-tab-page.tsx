import { t } from "@shared/i18n";

export function NewTabPage() {
  return (
    <div className="haku-page haku-page-centered">
      <h1 className="haku-page-title">{t("newTab.title")}</h1>
      <p className="haku-help">{t("newTab.prompt")}</p>
    </div>
  );
}
