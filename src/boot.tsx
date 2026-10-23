import { App } from "@app/app";
import { InterfaceCrash } from "@app/crash";
import { initTheme } from "@app/theme";
import { notifyFailure, reportUnexpected } from "@features/feedback/feedback";
import { onCommandFailure } from "@ipc/commands";
import { connect } from "@ipc/store";
import { ErrorBoundary } from "@shared/error-boundary";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

initTheme();
onCommandFailure(notifyFailure);

const container = document.querySelector("#root");
if (!container) {
  throw new Error("missing #root");
}

// State is loaded before the first paint so the interface never shows an empty
// tab strip and then snaps to the restored session. A failure here still
// renders: an interface reporting no tabs is more useful than a blank window.
try {
  await connect();
} catch (error) {
  reportUnexpected(error);
}

createRoot(container).render(
  <StrictMode>
    <ErrorBoundary fallback={(error) => <InterfaceCrash error={error} />}>
      <App />
    </ErrorBoundary>
  </StrictMode>,
);
