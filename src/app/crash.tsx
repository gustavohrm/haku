import { crashStyle, describeCrash } from "@app/crash-style";
import { useChromeLayout } from "@features/viewport/use-chrome-layout";
import { WindowControls } from "@features/window-controls/window-controls";
import { t } from "@shared/i18n";
import { useRef, type ReactNode } from "react";

/**
 * Shown in place of an internal page that failed to render, inside the
 * viewport, while the rest of the interface keeps working.
 */
export function PageCrash({ error, retry }: { error: unknown; retry: () => void }) {
  return (
    <CrashNotice
      title={t("error.page")}
      description={t("error.page.help")}
      error={error}
      action={
        <button type="button" className={crashStyle.action} onClick={retry}>
          {t("error.retry")}
        </button>
      }
    />
  );
}

/**
 * Shown when the interface itself failed to render: the window's frame, with
 * the notice on the page surface.
 *
 * The chrome is kept solid over the surface, as for an internal page, so a page
 * that was showing does not cover the notice. The window controls stay, since
 * the window has no frame of its own to close it with.
 */
export function InterfaceCrash({ error }: { error: unknown }) {
  const surfaceRef = useRef<HTMLElement>(null);
  useChromeLayout(surfaceRef, true);

  return (
    <div className={crashStyle.frame} data-density="compact">
      <header className={crashStyle.bar} data-tauri-drag-region>
        <WindowControls />
      </header>
      <main ref={surfaceRef} className={crashStyle.surface}>
        <CrashNotice
          title={t("error.interface")}
          description={t("error.interface.help")}
          error={error}
          action={
            <button type="button" className={crashStyle.action} onClick={() => globalThis.location.reload()}>
              {t("error.reloadInterface")}
            </button>
          }
        />
      </main>
    </div>
  );
}

interface CrashNoticeProps {
  title: string;
  description: string;
  action: ReactNode;
  error: unknown;
}

/** What failed, what to do about it, and the error itself, folded away. */
function CrashNotice({ title, description, action, error }: CrashNoticeProps) {
  return (
    <div className={crashStyle.page}>
      <div className={crashStyle.notice}>
        <i className={crashStyle.mark} aria-hidden="true" />
        <h1 className={crashStyle.title}>{title}</h1>
        <p className={crashStyle.description}>{description}</p>
        {action}
        <details className={crashStyle.details}>
          <summary className={crashStyle.summary}>{t("error.details")}</summary>
          <pre className={crashStyle.error}>{describeCrash(error)}</pre>
        </details>
      </div>
    </div>
  );
}
