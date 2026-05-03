interface TabOptions {
  id: string;
  title: string;
  url: string;
  favicon?: string;
}

function createTabElement(
  options: TabOptions,
  isActive: boolean,
  onSelect: () => void,
  onClose: () => void,
): HTMLButtonElement {
  const tab = document.createElement("button");
  const title = document.createElement("span");
  const closeButton = document.createElement("button");

  if (options.favicon) {
    const favicon = document.createElement("img");
    favicon.src = options.favicon;
    favicon.alt = "";
    tab.appendChild(favicon);
  } else {
    const icon = document.createElement("i");
    icon.className = "ic-web";
    tab.appendChild(icon);
  }

  tab.className = `tab${isActive ? " active" : ""}`;
  tab.type = "button";
  title.textContent = options.title;

  closeButton.type = "button";
  closeButton.className = "tab-close";
  closeButton.innerHTML = '<i class="ic-close"></i>';

  tab.appendChild(title);
  tab.appendChild(closeButton);
  tab.dataset.id = options.id;

  tab.addEventListener("click", onSelect);
  closeButton.addEventListener("click", (e) => {
    e.stopPropagation();
    onClose();
  });

  return tab;
}

class TabManager {
  private tabs: Map<string, HTMLButtonElement> = new Map();
  private activeTab: string | null = null;
  private container: HTMLElement | null = null;

  init(container: HTMLElement) {
    this.container = container;
  }

  getActiveTab() {
    return this.activeTab;
  }

  getTabs() {
    return this.tabs;
  }

  render(tabsData: TabOptions[], activeId: string, onSelect: (id: string) => void, onClose: (id: string) => void) {
    if (!this.container) return;

    this.container.replaceChildren();
    this.tabs.clear();
    this.activeTab = activeId;

    for (const tab of tabsData) {
      const el = createTabElement(
        tab,
        tab.id === activeId,
        () => onSelect(tab.id),
        () => onClose(tab.id),
      );
      this.tabs.set(tab.id, el);
      this.container.appendChild(el);
    }
  }

  newTab() {}
}

export default new TabManager();
