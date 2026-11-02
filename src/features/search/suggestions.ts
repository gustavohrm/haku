import { type Suggestion, type Suggestions } from "@bindings";
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
 * tab, an engine's keyword ready for a query, and for the first row what
 * Enter acts on.
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
  }
}
