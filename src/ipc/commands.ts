/**
 * The commands the interface calls, with failures reported instead of dropped.
 *
 * The generated bindings resolve a failure to a value rather than throwing, and
 * most call sites fire and forget, so a failed command would otherwise vanish
 * without a trace. Every command here passes its failure to one handler, which
 * the interface points at its notifications. Import from here rather than from
 * `@bindings` for anything that calls a command.
 *
 * Tauri rejects some calls itself, before Haku's code runs, such as one whose
 * arguments do not match: it rejects with a string, or with an `Error` the
 * bindings rethrow. Both are reported and handed back as a `Tauri` failure, so
 * every failure reads the same and none escapes a call nobody awaits.
 */

import { commands as generated, type HakuError } from "@bindings";

type FailureHandler = (error: HakuError) => void;

let handler: FailureHandler | null = null;

/**
 * Sets where command failures are reported.
 *
 * @param next - Receives every failure, or `null` to stop reporting.
 */
export function onCommandFailure(next: FailureHandler | null): void {
  handler = next;
}

function isFailure(value: unknown): value is { status: "error"; error: unknown } {
  return typeof value === "object" && value !== null && "status" in value && value.status === "error";
}

function asHakuError(error: unknown): HakuError {
  if (typeof error === "object" && error !== null && "kind" in error && typeof error.kind === "string") {
    return error as HakuError;
  }
  return { kind: "Tauri", message: error instanceof Error ? error.message : String(error) };
}

type AnyCommand = (...args: unknown[]) => Promise<unknown>;

export const commands = Object.fromEntries(
  Object.entries(generated).map(([name, command]) => [
    name,
    async (...args: unknown[]) => {
      let result: unknown;
      try {
        result = await (command as AnyCommand)(...args);
      } catch (thrown) {
        result = { status: "error", error: thrown };
      }
      if (!isFailure(result)) {
        return result;
      }
      const error = asHakuError(result.error);
      handler?.(error);
      return { status: "error", error };
    },
  ]),
) as typeof generated;
