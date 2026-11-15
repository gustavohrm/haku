import { describe, expect, it } from "vitest";

import { storeExtension } from "./store";

const ID = "ddkjiahejlhfcafbddmgiahcphecmpfh";

describe("storeExtension", () => {
  it("reads the id from a Chrome Web Store detail page, with or without its name", () => {
    const chrome = { store: "chrome", id: ID };
    expect(storeExtension(`https://chromewebstore.google.com/detail/ublock-origin-lite/${ID}`)).toEqual(chrome);
    expect(storeExtension(`https://chromewebstore.google.com/detail/${ID}`)).toEqual(chrome);
    expect(storeExtension(`https://chromewebstore.google.com/detail/${"a".repeat(32)}/${ID}`)).toEqual(chrome);
    expect(storeExtension(`https://chromewebstore.google.com/detail/name/${ID}?hl=en&pli=1#reviews`)).toEqual(chrome);
    expect(storeExtension(`https://chromewebstore.google.com/detail/name/${ID}/reviews`)).toEqual(chrome);
    expect(storeExtension(`https://chrome.google.com/webstore/detail/name/${ID}`)).toEqual(chrome);
  });

  it("reads the id from a Microsoft Edge Add-ons detail page, with or without its name", () => {
    const edge = { store: "edge", id: ID };
    expect(storeExtension(`https://microsoftedge.microsoft.com/addons/detail/ublock-origin/${ID}`)).toEqual(edge);
    expect(storeExtension(`https://microsoftedge.microsoft.com/addons/detail/${ID}`)).toEqual(edge);
    expect(storeExtension(`https://microsoftedge.microsoft.com/addons/detail/name/${ID}?hl=en-US`)).toEqual(edge);
  });

  it("finds nothing on any other page", () => {
    for (const url of [
      "https://chromewebstore.google.com/",
      "https://chromewebstore.google.com/category/extensions",
      `https://chromewebstore.google.com/detail/name/other/${ID}`,
      `https://chromewebstore.google.com/detail/name/${ID.toUpperCase()}`,
      `https://chromewebstore.google.com/detail/name/${ID.slice(1)}`,
      `http://chromewebstore.google.com/detail/name/${ID}`,
      `https://chromewebstore.google.com.example/detail/name/${ID}`,
      `https://example.com/detail/name/${ID}`,
      "https://microsoftedge.microsoft.com/addons/Microsoft-Edge-Extensions-Home",
      `https://microsoftedge.microsoft.com/detail/name/${ID}`,
      `http://microsoftedge.microsoft.com/addons/detail/name/${ID}`,
      `https://microsoftedge.microsoft.com.example/addons/detail/name/${ID}`,
      "haku://settings",
      "not a url",
    ]) {
      expect({ url, found: storeExtension(url) }).toEqual({ url, found: null });
    }
  });
});
