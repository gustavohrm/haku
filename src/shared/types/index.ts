export interface Website {
  title: string;
  url: string;
  favicon?: string;
  order: number;
}

export interface Tab {
  id: string;
  history: Website[];
  order: number;
}
