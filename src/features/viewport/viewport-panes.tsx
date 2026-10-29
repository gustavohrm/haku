import { PageCrash, TabCrash } from "@app/crash";
import { internalTitle, renderInternal } from "@app/router";
import type { Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { ErrorBoundary } from "@shared/error-boundary";
import { t } from "@shared/i18n";
import { Fragment, useEffect, useEffectEvent, useRef } from "react";

import { PreviewCover, usePreviewCover } from "./preview-cover";
import { SplitDivider } from "./split-divider";
import { useChromeLayout } from "./use-chrome-layout";

function pageName(tab: Tab): string {
  const visit = tab.history.entries[tab.history.index];
  return internalTitle(visit?.url ?? "") ?? (visit?.title.trim() || t("tabs.untitled"));
}

/** The window's assigned panes, measured together including their solid covers. */
export function ViewportPanes() {
  const { viewports, tabs, focusedViewport, splits, active, layoutRevision } = useBrowserState();
  const containerRef = useRef<HTMLDivElement>(null);
  useChromeLayout(containerRef);
  const isSplit = viewports.length === 2;
  const names = viewports
    .map((pane) => tabs.find((tab) => tab.id === pane.tab))
    .map((tab) => (tab ? pageName(tab) : t("tabs.untitled")));
  return (
    <main ref={containerRef} className={cx("mx-1 mb-1 grid min-h-0 min-w-0 flex-1", isSplit && "haku-split")}>
      {viewports.map((pane, index) => {
        const tab = tabs.find((candidate) => candidate.id === pane.tab);
        return (
          <Fragment key={pane.id}>
            {index === 1 && (
              <SplitDivider
                containerRef={containerRef}
                ratio={splits.find((split) => active !== null && split.tabs.includes(active))?.ratio ?? 0.5}
                tabId={viewports[0]!.tab}
                revision={layoutRevision}
                assignments={viewports.map((pane) => `${pane.id}:${pane.tab}`).join(",")}
                names={[names[0]!, names[1]!]}
              />
            )}
            {tab && (
              <Pane id={pane.id} tab={tab} isSplit={isSplit} isFocused={pane.id === focusedViewport} position={index} />
            )}
          </Fragment>
        );
      })}
    </main>
  );
}

/** Each pane owns its preview, internal route lifetime and error boundary. */
function Pane({
  id,
  tab,
  isSplit,
  isFocused,
  position,
}: {
  id: string;
  tab: Tab;
  isSplit: boolean;
  isFocused: boolean;
  position: number;
}) {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const isInternal = tab.presence.status === "internal";
  const crash = tab.presence.status === "crashed" ? tab.presence.cause : null;
  const url = tab.history.entries[tab.history.index]?.url ?? "";
  const cover = usePreviewCover(tab);
  const isCovered = isInternal || crash !== null || cover !== null;
  const title = pageName(tab);
  const focus = () => {
    if (!isFocused) {
      void commands.focusPane(tab.id);
    }
  };
  const focusSelected = useEffectEvent(() => {
    if (isFocused && isCovered && !surfaceRef.current?.contains(document.activeElement)) {
      surfaceRef.current?.focus({ preventScroll: true });
    }
  });
  useEffect(() => {
    focusSelected();
    // Only a new logical selection moves DOM focus; a preview or dialog update must not steal it.
  }, [isFocused, tab.id]);
  return (
    <section
      // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- A pane groups chrome and page content, not form controls.
      role="group"
      aria-label={`${isFocused ? t("split.current") : t(position === 0 ? "split.left" : "split.right")}: ${title}`}
      className={cx(
        "flex min-h-0 min-w-0 flex-col rounded-(--haku-viewport-radius)",
        isSplit && isFocused && "haku-pane-focused",
      )}
      onPointerDownCapture={focus}
      onFocusCapture={focus}
    >
      <div
        ref={surfaceRef}
        data-viewport={id}
        data-covered={isCovered}
        tabIndex={-1}
        className="haku-viewport relative min-h-0 min-w-0 flex-1 overflow-hidden outline-offset-[-2px]"
      >
        {isInternal && (
          <ErrorBoundary
            key={`${tab.id}:${url}`}
            fallback={(error, retry) => <PageCrash error={error} retry={retry} />}
          >
            {renderInternal(url, tab.id)}
          </ErrorBoundary>
        )}
        {crash !== null && <TabCrash tabId={tab.id} cause={crash} />}
        {cover !== null && <PreviewCover src={cover} />}
      </div>
    </section>
  );
}
