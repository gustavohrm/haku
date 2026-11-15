import { describe, expect, it } from "vitest";

import { storeExtensionId } from "./store";

const ID = "ddkjiahejlhfcafbddmgiahcphecmpfh";

describe("storeExtensionId", () => {
  it("reads the id from a detail page, with or without its name", () => {
    expect(storeExtensionId(`https://chromewebstore.google.com/detail/ublock-origin-lite/${ID}`)).toBe(ID);
    expect(storeExtensionId(`https://chromewebstore.google.com/detail/${ID}`)).toBe(ID);
    expect(storeExtensionId(`https://chromewebstore.google.com/detail/name/${ID}?hl=en&pli=1#reviews`)).toBe(ID);
    expect(storeExtensionId(`https://chromewebstore.google.com/detail/name/${ID}/reviews`)).toBe(ID);
    expect(storeExtensionId(`https://chrome.google.com/webstore/detail/name/${ID}`)).toBe(ID);
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
      "haku://settings",
      "not a url",
    ]) {
      expect({ url, id: storeExtensionId(url) }).toEqual({ url, id: null });
    }
  });
});
