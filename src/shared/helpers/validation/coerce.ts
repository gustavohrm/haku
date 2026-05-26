import { ok, err, Result } from "../result";

const DECIMAL_INTEGER_PATTERN = /^[+-]?\d+$/;
const DECIMAL_NUMBER_PATTERN = /^[+-]?(?:\d+\.?\d*|\.\d+)$/;

const stringify = (val: unknown, opts: { primitivesOnly?: boolean } = {}): Result<string> => {
  if (opts.primitivesOnly && (typeof val === "object" || typeof val === "function")) {
    return err("Cannot convert value to string");
  }
  try {
    return ok(String(val));
  } catch {
    return err("Cannot convert value to string");
  }
};

export const coerce = {
  int(val: unknown): Result<number> {
    const stringResult = stringify(val, { primitivesOnly: true });
    if (!stringResult.ok) return err("Cannot coerce value to integer");

    const s = stringResult.value.trim();
    if (!DECIMAL_INTEGER_PATTERN.test(s)) return err(`Cannot coerce "${stringResult.value}" to integer`);

    const n = Number(s);
    if (s.length === 0 || !Number.isSafeInteger(n)) return err(`Cannot coerce "${stringResult.value}" to integer`);
    return ok(n);
  },

  number(val: unknown): Result<number> {
    const stringResult = stringify(val, { primitivesOnly: true });
    if (!stringResult.ok) return err("Cannot coerce value to number");

    const s = stringResult.value.trim();
    if (!DECIMAL_NUMBER_PATTERN.test(s)) return err(`Cannot coerce "${stringResult.value}" to number`);

    const n = Number(s);
    if (s.length === 0 || !Number.isFinite(n)) return err(`Cannot coerce "${stringResult.value}" to number`);
    return ok(n);
  },

  bool(val: unknown): Result<boolean> {
    if (typeof val === "boolean") return ok(val);

    const stringResult = stringify(val, { primitivesOnly: true });
    if (!stringResult.ok) return err("Cannot coerce value to boolean");

    const s = stringResult.value.toLowerCase().trim();
    if (["true", "1", "yes", "on"].includes(s)) return ok(true);
    if (["false", "0", "no", "off"].includes(s)) return ok(false);
    return err(`Cannot coerce "${stringResult.value}" to boolean`);
  },

  string(val: unknown): Result<string> {
    if (val === null || val === undefined) return err("Cannot coerce null or undefined to string");

    const stringResult = stringify(val, { primitivesOnly: true });
    if (!stringResult.ok) return err("Cannot coerce value to string");

    return stringResult;
  },
};
