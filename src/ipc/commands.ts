/**
 * The commands the interface calls, with failures reported instead of dropped.
 *
 * The generated bindings resolve a failure to a value rather than throwing, and
 * most call sites fire and forget, so a failed command would otherwise vanish
 * without a trace. Every command here passes its failure to one handler, which
 * the interface points at its notifications. Import from here rather than from
 * `@bindings` for anything that calls a command.
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

function isFailure(value: unknown): value is { status: "error"; error: HakuError } {
  return typeof value === "object" && value !== null && "status" in value && value.status === "error";
}

type AnyCommand = (...args: unknown[]) => Promise<unknown>;

export const commands = Object.fromEntries(
  Object.entries(generated).map(([name, command]) => [
    name,
    async (...args: unknown[]) => {
      const result = await (command as AnyCommand)(...args);
      if (isFailure(result)) {
        handler?.(result.error);
      }
      return result;
    },
  ]),
) as typeof generated;
