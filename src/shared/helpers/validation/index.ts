export { ok, err } from "../result";
export type { Result, Ok, Err } from "../result";

export { coerce } from "./coerce";
export { custom } from "./custom";

import { string } from "./string";
import { number } from "./number";

export const val = { string, number };
