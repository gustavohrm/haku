import { useBrowserState } from "@ipc/hooks";
import { t, type TranslationKey } from "@shared/i18n";

export function NewTabPage() {
  const { private: isPrivate } = useBrowserState();
  if (isPrivate) {
    return <PrivateNewTabPage />;
  }
  return (
    <div className="bg-background grid h-full place-content-center gap-2 text-center select-text">
      <h1 className="text-title">{t("newTab.title")}</h1>
      <p className="text-text-secondary">{t("newTab.prompt")}</p>
    </div>
  );
}

const NOT_KEPT: TranslationKey[] = ["private.notKept.history", "private.notKept.siteData", "private.notKept.tabs"];
const VISIBLE_TO: TranslationKey[] = ["private.visibleTo.sites", "private.visibleTo.network"];

/** What a private window keeps and what it cannot hide, as other browsers say it. */
function PrivateNewTabPage() {
  return (
    <div className="bg-background grid h-full place-content-center overflow-auto p-8 select-text">
      <div className="flex max-w-2xl flex-col items-center gap-2 text-center">
        <div className="bg-surface text-text mb-2 grid size-16 place-items-center rounded-full">
          <i className="ic-hat-glasses [--ic-size:2rem]" aria-hidden="true" />
        </div>
        <h1 className="text-title">{t("private.title")}</h1>
        <p className="text-text-secondary">{t("newTab.prompt")}</p>
        <div className="mt-6 grid w-full gap-3 text-left sm:grid-cols-2">
          <PrivateFacts heading="private.notKept" items={NOT_KEPT} />
          <PrivateFacts heading="private.visibleTo" items={VISIBLE_TO} />
        </div>
        <p className="text-text-secondary mt-4 flex items-center gap-1.5 text-sm">
          <i className="ic-blocks ic-sm shrink-0" aria-hidden="true" />
          {t("private.extensions")}
        </p>
      </div>
    </div>
  );
}

function PrivateFacts({ heading, items }: { heading: TranslationKey; items: TranslationKey[] }) {
  return (
    <section className="bg-surface rounded-(--radius-control) p-4">
      <h2 className="text-text mb-2 font-medium">{t(heading)}</h2>
      <ul className="text-text-secondary flex list-disc flex-col gap-1 pl-5 text-sm">
        {items.map((item) => (
          <li key={item}>{t(item)}</li>
        ))}
      </ul>
    </section>
  );
}
