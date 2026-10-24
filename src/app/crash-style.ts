/**
 * Classes of the screens shown when something fails to render or to start.
 *
 * Shared by the React screens in `crash.tsx`, the settings page when its
 * settings could not be loaded, and the startup failure screen, which is built
 * without React because React may be what failed. The frame
 * matches the interface's own: a drag bar holding the window controls, around
 * the rounded page surface.
 */
export const crashStyle = {
  frame: "flex h-full flex-col",
  bar: "haku-bar flex h-8.5 shrink-0 items-center justify-end",
  controls: "flex shrink-0 items-stretch self-stretch",
  surface: "haku-viewport relative mx-1 mb-1 min-h-0 flex-1 overflow-hidden",
  page: "bg-background grid h-full place-content-center overflow-auto p-8 select-text",
  notice: "flex max-w-md flex-col items-center gap-2 text-center",
  mark: "ic-haku ic-xl mb-2 text-text-secondary",
  title: "text-title",
  description: "text-text-secondary",
  action: "btn mt-4",
  details: "mt-6 w-full text-text-secondary",
  summary: "cursor-pointer",
  error:
    "mt-3 max-h-48 overflow-auto rounded-(--radius-control) bg-surface p-3 text-left font-mono text-xs break-all whitespace-pre-wrap [scrollbar-width:thin]",
} as const;

/**
 * What was thrown, as the details of a crash screen show it.
 *
 * Chromium's stack opens with the error's name and message; WebKit's is the
 * frames alone, so the message is put in front when the stack lacks it.
 *
 * @param error - What was thrown.
 * @returns The name and message, then the stack when there is one.
 */
export function describeCrash(error: unknown): string {
  const summary = String(error);
  if (!(error instanceof Error) || !error.stack) {
    return summary;
  }
  return error.stack.includes(summary) ? error.stack : `${summary}\n${error.stack}`;
}
