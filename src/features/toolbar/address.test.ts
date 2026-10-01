import { describe, expect, it } from "vitest";

import { addressIcon, displayAddress } from "./address";

describe("displayAddress", () => {
  it("drops the secure scheme and a bare trailing slash", () => {
    expect(displayAddress("https://tabnews.com.br/")).toBe("tabnews.com.br");
  });

  it("keeps the path, including a trailing slash that belongs to it", () => {
    expect(displayAddress("https://a.test/docs/")).toBe("a.test/docs/");
  });

  it("keeps an insecure scheme visible", () => {
    expect(displayAddress("http://a.test/")).toBe("http://a.test/");
  });

  it("leaves internal pages alone", () => {
    expect(displayAddress("haku:settings")).toBe("haku:settings");
  });
});

describe("addressIcon", () => {
  it("shows a search glyph while typing, whatever the page is", () => {
    expect(addressIcon("https://a.test", true)).toBe("ic-search");
  });

  it("states whether the connection is secure", () => {
    expect(addressIcon("https://a.test", false)).toBe("ic-lock");
    expect(addressIcon("http://a.test", false)).toBe("ic-lock-open");
  });
});
