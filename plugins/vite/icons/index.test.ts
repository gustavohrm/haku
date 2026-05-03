import { describe, expect, it } from "vitest";

import iconsPlugin from "./index";

interface MinimalIndexHtmlTransformContext {
  path: string;
  filename: string;
}

type IndexHtmlHandler = (this: unknown, html: string, context: MinimalIndexHtmlTransformContext) => unknown;
type TransformHandler = (this: unknown, code: string, id: string) => unknown;

function getTransformIndexHtmlHandler(): IndexHtmlHandler {
  const plugin = iconsPlugin();
  const { transformIndexHtml } = plugin;

  if (!transformIndexHtml) {
    throw new Error("iconsPlugin transformIndexHtml handler is not available");
  }

  if (typeof transformIndexHtml === "function") {
    return transformIndexHtml as IndexHtmlHandler;
  }

  return transformIndexHtml.handler as IndexHtmlHandler;
}

function getTransformHook(): TransformHandler {
  const plugin = iconsPlugin();
  const { transform } = plugin;

  if (!transform) {
    throw new Error("iconsPlugin transform hook is not available");
  }

  if (typeof transform === "function") {
    return transform as TransformHandler;
  }

  return transform.handler as TransformHandler;
}

async function runTransformIndexHtml(html: string): Promise<string> {
  const transformIndexHtml = getTransformIndexHtmlHandler();
  const context: MinimalIndexHtmlTransformContext = { path: "/index.html", filename: "index.html" };
  const transformed = await transformIndexHtml.call({}, html, context);

  if (typeof transformed === "string") {
    return transformed;
  }

  throw new Error("iconsPlugin transformIndexHtml handler returned a non-string result");
}

async function runTransform(code: string, id: string): Promise<{ code: string; map: null } | null> {
  const transform = getTransformHook();
  const transformed = await transform.call({}, code, id);

  if (transformed === null || transformed === undefined) {
    return null;
  }

  if (typeof transformed === "string") {
    return { code: transformed, map: null };
  }

  if (typeof transformed === "object" && "code" in transformed && typeof transformed.code === "string") {
    return { code: transformed.code, map: null };
  }

  throw new Error("iconsPlugin transform hook returned an unsupported result");
}

describe("iconsPlugin", () => {
  it("should move non-icon classes from the marker element to the rendered svg", async () => {
    const transformed = await runTransformIndexHtml(`<div><i class="ic-success size-4 text-green-500"></i></div>`);

    expect(transformed).toContain(`<svg class="size-4 text-green-500"`);
    expect(transformed).not.toContain("ic-success");
  });

  it("should keep passthrough attributes around class", async () => {
    const transformed = await runTransformIndexHtml(
      `<div><i id="status" class="ic-info size-4" aria-hidden="true"></i></div>`,
    );

    expect(transformed).toContain(`<svg class="size-4" id="status" aria-hidden="true"`);
  });

  it("should consume explicit closing icon marker tags", async () => {
    const transformed = await runTransformIndexHtml(`<button><i class="ic-sun block"></i></button>`);

    expect(transformed).toContain(`<svg class="block"`);
    expect(transformed).not.toContain("</i>");
  });

  it("should replace when a valid icon marker appears after an invalid marker", async () => {
    const transformed = await runTransformIndexHtml(`<div><i class="ic-missing text-red ic-error"></i></div>`);

    expect(transformed).toContain(`<svg class="ic-missing text-red"`);
    expect(transformed).not.toContain("<i class=");
  });

  it("should keep html unchanged for unknown icon names", async () => {
    const html = `<div><i class="ic-does-not-exist text-blue"></i></div>`;

    expect(await runTransformIndexHtml(html)).toBe(html);
  });

  it("should not leak regex state across calls", async () => {
    const first = await runTransformIndexHtml(`<div><i class="ic-success"></i></div>`);
    const second = await runTransformIndexHtml(`<div><i class="ic-error"></i></div>`);

    expect(first).toContain("<svg");
    expect(second).toContain("<svg");
  });

  it("should transform JS/TS files and ignore unsupported extensions", async () => {
    const supported = await runTransform(`<i class="ic-warning utility"></i>`, "entry.ts?import");
    const unsupported = await runTransform(`<i class="ic-warning utility"></i>`, "styles.css");

    expect(supported).toMatchObject({ map: null });
    expect(supported?.code ?? "").toContain(`<svg class="utility"`);
    expect(unsupported).toBeNull();
  });
});
