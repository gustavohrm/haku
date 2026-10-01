import type { HakuError } from "@bindings";

/** The shape every generated command resolves to. */
export type CommandResult<T> = { status: "ok"; data: T } | { status: "error"; error: HakuError };

/**
 * A failure that came from Rust, carrying the typed variant alongside a message.
 *
 * Wrapping rather than throwing the raw union keeps `catch` blocks working with
 * an ordinary `Error` while leaving the discriminant available to anything that
 * wants to branch on it.
 */
export class HakuFailure extends Error {
  readonly cause: HakuError;

  constructor(cause: HakuError) {
    super(describeError(cause));
    this.name = "HakuFailure";
    this.cause = cause;
  }
}

/**
 * A human-readable account of a failure.
 *
 * @param error - The failure Rust reported.
 * @returns A sentence describing it.
 */
export function describeError(error: HakuError): string {
  switch (error.kind) {
    case "NoSlotAvailable":
      return "No webview slot is available.";
    case "TabNotFound":
      return `Tab not found: ${error.message}`;
    case "InvalidUrl":
      return `Invalid URL: ${error.message}`;
    case "Unsupported":
      return `Unsupported: ${error.message}`;
    case "WindowMissing":
      return `Window or webview missing: ${error.message}`;
    case "Storage":
      return `Storage failure: ${error.message}`;
    case "Tauri":
      return `Tauri failure: ${error.message}`;
  }
}

/**
 * Unwraps a command result, throwing a {@link HakuFailure} on error.
 *
 * @param result - What a generated command resolved to.
 * @returns The command's value.
 * @throws {HakuFailure} When the command reported a failure.
 */
export async function unwrap<T>(result: Promise<CommandResult<T>>): Promise<T> {
  const settled = await result;
  if (settled.status === "error") {
    throw new HakuFailure(settled.error);
  }
  return settled.data;
}
