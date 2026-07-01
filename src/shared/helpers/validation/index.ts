export { ok, err } from "../result";
export type { Result, Ok, Err } from "../result";

export { coerce } from "./coerce";
export { custom } from "./custom";

import { number } from "./number";
import { string } from "./string";

export const val = { string, number };
