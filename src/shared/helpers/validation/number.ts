import { ok, err, Result } from "../result";

interface NumberValidators {
  positive(): Result<number>;
  negative(): Result<number>;
  nonZero(): Result<number>;
  int(): Result<number>;
  range(opts: { min?: number; max?: number }): Result<number>;
  finite(): Result<number>;
  port(): Result<number>;
}

export function number(val: unknown): NumberValidators {
  if (typeof val !== "number" || isNaN(val)) {
    const typeErr = err(`Expected number, got ${typeof val}`);
    const fail = () => typeErr;
    return {
      positive: fail,
      negative: fail,
      nonZero: fail,
      int: fail,
      range: fail,
      finite: fail,
      port: fail,
    };
  }

  if (!Number.isFinite(val)) {
    const finiteErr = err("Must be a finite number");
    const fail = () => finiteErr;
    return {
      positive: fail,
      negative: fail,
      nonZero: fail,
      int: fail,
      range: fail,
      finite: fail,
      port: fail,
    };
  }

  const v = val;

  return {
    positive(): Result<number> {
      if (v <= 0) return err("Must be a positive number");
      return ok(v);
    },

    negative(): Result<number> {
      if (v >= 0) return err("Must be a negative number");
      return ok(v);
    },

    nonZero(): Result<number> {
      if (v === 0) return err("Must not be zero");
      return ok(v);
    },

    int(): Result<number> {
      if (!Number.isInteger(v)) return err("Must be an integer");
      return ok(v);
    },

    range({ min, max }: { min?: number; max?: number }): Result<number> {
      if (min !== undefined && !Number.isFinite(min)) return err("Range minimum must be a finite number");
      if (max !== undefined && !Number.isFinite(max)) return err("Range maximum must be a finite number");
      if (min !== undefined && max !== undefined && min > max)
        return err("Range minimum cannot be greater than maximum");
      if (min !== undefined && v < min) return err(`Must be at least ${min}`);
      if (max !== undefined && v > max) return err(`Must be at most ${max}`);
      return ok(v);
    },

    finite(): Result<number> {
      if (!Number.isFinite(v)) return err("Must be a finite number");
      return ok(v);
    },

    port(): Result<number> {
      if (!Number.isInteger(v) || v < 1 || v > 65535) return err("Must be a valid port number (1–65535)");
      return ok(v);
    },
  };
}
