interface TabOptions {
  id: string;
  title: string;
  url: string;
  favicon?: string;
}

function createTabElement(options: TabOptions): HTMLButtonElement {
  const tab = document.createElement("button");
  const title = document.createElement("span");
  const closeButton = document.createElement("button");

  if (options.favicon) {
    const favicon = document.createElement("img");
    favicon.src = options.favicon;
    favicon.alt = "";
    favicon.className = "size-4";
    tab.appendChild(favicon);
  }

  tab.className = "tab";
  title.textContent = options.title;

  closeButton.className = "tab-close";
  closeButton.innerHTML = '<i class="ic-close"></i>';

  tab.appendChild(title);
  tab.appendChild(closeButton);

  tab.dataset.id = options.id;
  return tab;
}

class TabManager {
  private tabs: Map<string, HTMLButtonElement>;
  private activeTab: string | null;

  public getActiveTab() {}
  public getTabs() {}
  public newTab() {}
}

export default new TabManager();
