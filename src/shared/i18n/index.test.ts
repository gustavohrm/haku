import { afterEach, describe, expect, it } from "vitest";

import { DEFAULT_LOCALE, locale, setLocale, t } from "./index";

afterEach(() => {
  setLocale(DEFAULT_LOCALE);
});

describe("translations", () => {
  it("translates a known key", () => {
    expect(t("tabs.new")).toBe("New tab");
  });

  it("starts on the default locale", () => {
    expect(locale()).toBe(DEFAULT_LOCALE);
  });

  it("falls back to the default locale when one is not available yet", () => {
    expect(setLocale("pt")).toBe(DEFAULT_LOCALE);
    expect(t("tabs.new")).toBe("New tab");
  });

  it("reports the locale actually in use rather than the one requested", () => {
    setLocale("es");

    expect(locale()).toBe(DEFAULT_LOCALE);
  });

  it("switches back to a locale that does exist", () => {
    setLocale("pt");

    expect(setLocale("en")).toBe("en");
  });
});
