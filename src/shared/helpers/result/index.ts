import AppError, { type AppErrorOptions } from "../error";

export type Ok<T> = { ok: true; value: T };
export type Err = { ok: false; error: AppError };
export type Result<T> = Ok<T> | Err;

export const ok = <T>(value: T): Ok<T> => ({ ok: true, value });
export const err = (error: unknown, options: AppErrorOptions = {}): Err => ({
  ok: false,
  error: new AppError(error, typeof error === "string" ? { fallbackMessage: error, ...options } : options),
});
