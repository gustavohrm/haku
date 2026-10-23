import type { ReactNode } from "react";

interface InternalPageProps {
  title: string;
  /** Controls shown beside the title, such as a page-wide action. */
  actions?: ReactNode;
  /**
   * Navigation beside the content, as settings has. It sits on the left from
   * 48 rem of page width, and above the content below that, so a narrow pane
   * still works.
   */
  sidebar?: ReactNode;
  children: ReactNode;
}

/**
 * The frame every internal page renders inside.
 *
 * Internal pages are drawn by the chrome in the viewport, where a web page
 * would otherwise show through, so they supply their own surface.
 */
export function InternalPage({ title, actions, sidebar, children }: InternalPageProps) {
  const content = (
    <>
      <header className="flex items-center justify-between gap-4">
        <h1 className="text-title">{title}</h1>
        {actions}
      </header>
      {children}
    </>
  );

  if (!sidebar) {
    return (
      <div className="bg-background h-full overflow-auto select-text">
        <div className="stack mx-auto max-w-3xl px-8 py-10">{content}</div>
      </div>
    );
  }
  return (
    <div className="bg-background @container h-full overflow-auto select-text">
      <div className="mx-auto flex max-w-5xl flex-col gap-6 px-6 py-8 @3xl:flex-row @3xl:gap-10 @3xl:px-8 @3xl:py-10">
        <aside className="flex shrink-0 flex-col gap-3 @3xl:sticky @3xl:top-10 @3xl:w-52 @3xl:self-start">
          {sidebar}
        </aside>
        <div className="stack max-w-2xl min-w-0 flex-1">{content}</div>
      </div>
    </div>
  );
}
