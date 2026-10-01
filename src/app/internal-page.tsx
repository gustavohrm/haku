import type { ReactNode } from "react";

interface InternalPageProps {
  title: string;
  /** Controls shown beside the title, such as a page-wide action. */
  actions?: ReactNode;
  children: ReactNode;
}

/**
 * The frame every internal page renders inside.
 *
 * Internal pages are drawn by the chrome in the viewport, where a web page
 * would otherwise show through, so they supply their own surface.
 */
export function InternalPage({ title, actions, children }: InternalPageProps) {
  return (
    <div className="bg-background h-full overflow-auto select-text">
      <div className="stack mx-auto max-w-3xl px-8 py-10">
        <header className="flex items-center justify-between gap-4">
          <h1 className="text-title">{title}</h1>
          {actions}
        </header>
        {children}
      </div>
    </div>
  );
}
