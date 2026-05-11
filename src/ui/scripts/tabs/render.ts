import type { ManagedTab } from "@shared/types";

interface RenderTabsOptions {
  tabs: ManagedTab[];
  selectedTabId: string;
  tabStrip: HTMLElement;
  onSelect(tabId: string): void;
  onClose(tabId: string): void;
}

function cloneFaviconFallback(): Element {
  const template = document.querySelector<HTMLTemplateElement>("#favicon-fallback-template");
  const fallback = template?.content.firstElementChild?.cloneNode(true);

  if (!(fallback instanceof Element)) {
    throw new Error("Favicon fallback template is missing.");
  }

  return fallback;
}

function createFavicon(tab: ManagedTab): HTMLElement {
  const container = document.createElement("span");
  const fallback = cloneFaviconFallback();

  container.className = "tab-favicon";
  container.append(fallback);

  if (!tab.current.favicon) {
    return container;
  }

  const image = document.createElement("img");
  image.src = tab.current.favicon;
  image.alt = "";
  image.addEventListener("load", () => fallback.toggleAttribute("hidden", true));
  image.addEventListener("error", () => image.remove());
  container.append(image);

  return container;
}

export function renderTabs({ tabs, selectedTabId, tabStrip, onSelect, onClose }: RenderTabsOptions): void {
  tabStrip.replaceChildren(
    ...tabs.map((tab) => {
      const tabElement = document.createElement("button");
      const title = document.createElement("span");
      const closeButton = document.createElement("button");

      tabElement.className = tab.id === selectedTabId ? "tab active" : "tab";
      tabElement.type = "button";
      tabElement.title = tab.current.title;
      tabElement.addEventListener("click", () => onSelect(tab.id));

      title.textContent = tab.current.title;

      closeButton.type = "button";
      closeButton.ariaLabel = "Close tab";
      closeButton.innerHTML = '<i class="ic-close"></i>';
      closeButton.addEventListener("click", (event) => {
        event.stopPropagation();
        onClose(tab.id);
      });

      tabElement.append(createFavicon(tab), title, closeButton);

      return tabElement;
    }),
  );
}
