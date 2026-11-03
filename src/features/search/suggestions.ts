import { type EngineSuggestions, type Suggestion, type Suggestions } from "@bindings";
import { commands } from "@ipc/commands";
import { useEffect, useState } from "react";

/*
  What the address bar suggests while the user types. Rust ranks the rows;
  this asks for them and says what each puts in the field.
  [Address bar § Suggestions](../../../docs/specs/address-bar.md#suggestions).
*/

/**
 * The suggestions for `input`, or null while there is no input or no answer
 * for it yet. An answer for input the field no longer shows is dropped.
 *
 * @param input - What the user typed, or null while they have not typed.
 */
export function useSuggestions(input: string | null): Suggestions | null {
  const [answer, setAnswer] = useState<Suggestions | null>(null);
  useEffect(() => {
    if (input === null || input.trim() === "") {
      return;
    }
    let isCurrent = true;
    const load = async () => {
      const result = await commands.suggest(input);
      if (isCurrent && result.status === "ok") {
        setAnswer(result.data);
      }
    };
    void load();
    return () => {
      isCurrent = false;
    };
  }, [input]);
  return answer !== null && input !== null && answer.input === input ? answer : null;
}

/** How long typing must pause before the search engine is asked, so it is not asked for every keystroke. */
export const ENGINE_SUGGESTIONS_DELAY_MS = 150;

/** The most rows the list shows, as Rust caps its own. */
const ROWS_MAX = 10;

/**
 * The search engine's completions for `input`, asked once typing pauses.
 *
 * The last answer stays until the next arrives, as the local rows do, so the
 * list does not flicker while the engine is asked; it is dropped once the
 * edit ends. Rust answers none while they are off, in a private window, or
 * for an address.
 *
 * @param input - What the user typed, or null while they have not typed.
 */
export function useEngineSuggestions(input: string | null): Suggestion[] {
  const [answer, setAnswer] = useState<EngineSuggestions | null>(null);
  const isBlank = input === null || input.trim() === "";
  if (input === null && answer !== null) {
    setAnswer(null);
  }
  useEffect(() => {
    if (input === null || input.trim() === "") {
      return;
    }
    let isCurrent = true;
    const load = async () => {
      const result = await commands.searchSuggestions(input);
      if (isCurrent && result.status === "ok") {
        setAnswer(result.data);
      }
    };
    const timer = window.setTimeout(() => void load(), ENGINE_SUGGESTIONS_DELAY_MS);
    return () => {
      isCurrent = false;
      window.clearTimeout(timer);
    };
  }, [input]);
  return isBlank ? [] : (answer?.rows ?? []);
}

/**
 * The local rows, then the engine's, at most {@link ROWS_MAX}. The engine's
 * take their room from the end of the local rows, which is the pages.
 */
export function withEngineRows(local: Suggestion[], engine: Suggestion[]): Suggestion[] {
  return [...local.slice(0, Math.max(ROWS_MAX - engine.length, 1)), ...engine].slice(0, ROWS_MAX);
}

/**
 * `typed` continued to `completion`, keeping the case it was typed in, or
 * null when the completion does not continue it.
 */
export function completed(typed: string, completion: string | null): string | null {
  if (completion === null || completion.length <= typed.length) {
    return null;
  }
  if (!completion.toLowerCase().startsWith(typed.toLowerCase())) {
    return null;
  }
  return typed + completion.slice(typed.length);
}

/**
 * What the field shows while a row is selected: the address of a page or a
 * tab, an engine's keyword ready for a query, what an engine's completion
 * searches with, and for the first row what Enter acts on.
 *
 * @param row - The selected row.
 * @param entered - What the first row acts on: the input, completed or not.
 */
export function rowText(row: Suggestion, entered: string): string {
  switch (row.kind) {
    case "search":
    case "visit": {
      return entered;
    }
    case "tab":
    case "page": {
      return row.url;
    }
    case "engine": {
      return `${row.keyword} `;
    }
    case "suggested": {
      return row.input;
    }
  }
}
