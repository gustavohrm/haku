import { t } from "@shared/i18n";

/**
 * The persistent marker of a private window, beside the address in browser and
 * popup bars. Labelled as well as drawn, so it never depends on recognising the
 * icon.
 */
export function PrivateBadge() {
  return (
    <span
      className="bg-chrome-raised text-text flex shrink-0 items-center gap-1.5 rounded-(--radius-control) px-2 py-1 font-medium"
      title={t("private.badge")}
    >
      <i className="ic-hat-glasses ic-md" aria-hidden="true" />
      {t("private.label")}
    </span>
  );
}
