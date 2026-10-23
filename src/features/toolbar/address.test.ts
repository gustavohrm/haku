import { describe, expect, it } from "vitest";

import { connectionIcon, displayAddress, editableAddress, isInsecure } from "./address";

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
    expect(displayAddress("haku://settings")).toBe("haku://settings");
  });

  it("shows nothing for the new-tab page", () => {
    expect(displayAddress("haku://new-tab")).toBe("");
  });
});

describe("editableAddress", () => {
  it("starts an edit from the full URL", () => {
    expect(editableAddress("https://a.test/")).toBe("https://a.test/");
  });

  it("starts an edit on the new-tab page from nothing", () => {
    expect(editableAddress("haku://new-tab")).toBe("");
  });
});

describe("isInsecure", () => {
  it("warns about plain HTTP only", () => {
    expect(isInsecure("http://a.test")).toBe(true);
    expect(isInsecure("https://a.test")).toBe(false);
    expect(isInsecure("haku://settings")).toBe(false);
  });
});

describe("connectionIcon", () => {
  it("states whether the connection is secure", () => {
    expect(connectionIcon("https://a.test")).toBe("ic-lock");
    expect(connectionIcon("http://a.test")).toBe("ic-lock-open");
    expect(connectionIcon("chrome-extension://abc/popup.html")).toBe("ic-globe");
  });
});
