import type { Result } from "../result";
import { ok, err } from "../result";

const PUBLIC_HOST_PATTERN = /^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,}$/i;
const EMAIL_LOCAL_PATTERN = /^[a-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_`{|}~-]+)*$/i;
const EMAIL_LOCAL_MAX_LENGTH = 64;
const EMAIL_MAX_LENGTH = 254;

const isValidLengthLimit = (n: number): boolean => Number.isFinite(n) && n >= 0;

interface StringValidators {
  email(opts?: { allowPlus?: boolean }): Result<string>;
  url(opts?: { forceHttps?: boolean }): Result<string>;
  minLength(n: number, opts?: { trim?: boolean }): Result<string>;
  maxLength(n: number, opts?: { trim?: boolean }): Result<string>;
  notEmpty(opts?: { trim?: boolean }): Result<string>;
  matches(pattern: RegExp, message?: string): Result<string>;
}

export function string(val: unknown): StringValidators {
  if (typeof val !== "string") {
    const typeErr = err(`Expected string, got ${typeof val}`);
    const fail = () => typeErr;
    return {
      email: fail,
      url: fail,
      minLength: fail,
      maxLength: fail,
      notEmpty: fail,
      matches: fail,
    };
  }

  const v = val;

  return {
    email({ allowPlus = true } = {}): Result<string> {
      const trimmed = v.trim();
      const [local, host, extra] = trimmed.split("@");
      if (extra !== undefined || local === undefined || host === undefined) {return err("Invalid email address");}
      if (trimmed.length > EMAIL_MAX_LENGTH || local.length > EMAIL_LOCAL_MAX_LENGTH)
        {return err("Invalid email address");}
      if (!PUBLIC_HOST_PATTERN.test(host)) {return err("Invalid email address");}
      if (!EMAIL_LOCAL_PATTERN.test(local)) {return err("Invalid email address");}
      if (!allowPlus && local.includes("+")) {return err("Invalid email address");}
      return ok(`${local}@${host.toLowerCase()}`);
    },

    url({ forceHttps = false } = {}): Result<string> {
      let s = v.trim();
      if (!/^https?:\/\//i.test(s)) {s = "https://" + s;}
      try {
        const url = new URL(s);
        if (!PUBLIC_HOST_PATTERN.test(url.hostname)) {return err("Invalid URL");}
        if (url.username.length > 0 || url.password.length > 0) {return err("Invalid URL");}

        if (forceHttps && url.protocol !== "https:") {return err("Invalid URL");}
        return ok(url.href);
      } catch {
        return err("Invalid URL");
      }
    },

    minLength(n: number, { trim = false } = {}): Result<string> {
      if (!isValidLengthLimit(n)) {return err("Minimum length must be a finite non-negative number");}

      const s = trim ? v.trim() : v;
      if (s.length < n) {return err(`Must be at least ${n} characters`);}
      return ok(s);
    },

    maxLength(n: number, { trim = false } = {}): Result<string> {
      if (!isValidLengthLimit(n)) {return err("Maximum length must be a finite non-negative number");}

      const s = trim ? v.trim() : v;
      if (s.length > n) {return err(`Must be at most ${n} characters`);}
      return ok(s);
    },

    notEmpty({ trim = true } = {}): Result<string> {
      const s = trim ? v.trim() : v;
      if (s.length === 0) {return err("Value cannot be empty");}
      return ok(s);
    },

    matches(pattern: RegExp, message = "Value does not match the required format"): Result<string> {
      const safePattern = new RegExp(pattern.source, pattern.flags);
      if (!safePattern.test(v)) {return err(message);}
      return ok(v);
    },
  };
}
