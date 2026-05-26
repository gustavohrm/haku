import { err, Result } from "../result";

export function custom<T>(val: unknown, fn: (val: unknown) => Result<T>): Result<T> {
  try {
    return fn(val);
  } catch (error) {
    return err(error instanceof Error ? error.message : String(error));
  }
}
