import { describe, expect, it } from "vitest";
import AppError from "../error";
import type { Result } from "../result";
import { coerce, custom, val } from "./index";

const expectErr = (result: Result<unknown>, message: string): void => {
  expect(result.ok).toBe(false);

  if (result.ok) {
    return;
  }

  expect(result.error).toBeInstanceOf(AppError);
  expect(result.error.message).toBe(message);
};

describe("validation helper", () => {
  describe("coerce.int", () => {
    it.each(["12abc", "1.9", "0x10", "1e3", "0b10"])("should reject non-integer input %s", (input) => {
      expectErr(coerce.int(input), `Cannot coerce "${input}" to integer`);
    });

    it("should reject integers outside the safe range", () => {
      expectErr(coerce.int("9007199254740993"), 'Cannot coerce "9007199254740993" to integer');
    });

    it.each([["1"], { toString: () => "1" }])("should reject non-primitive input %#", (input) => {
      expectErr(coerce.int(input), "Cannot coerce value to integer");
    });
  });

  describe("coerce.number", () => {
    it.each(["1abc", "0x10", "1e3", "0b10"])("should reject non-decimal input %s", (input) => {
      expectErr(coerce.number(input), `Cannot coerce "${input}" to number`);
    });

    it.each([["1.5"], { toString: () => "1.5" }])("should reject non-primitive input %#", (input) => {
      expectErr(coerce.number(input), "Cannot coerce value to number");
    });
  });

  describe("coerce", () => {
    it.each([
      ["int", coerce.int, "Cannot coerce value to integer"],
      ["number", coerce.number, "Cannot coerce value to number"],
      ["bool", coerce.bool, "Cannot coerce value to boolean"],
      ["string", coerce.string, "Cannot coerce value to string"],
    ] as const)("should return an error when %s cannot stringify input", (_, validator, error) => {
      expectErr(validator(Object.create(null)), error);
    });

    it.each([["true"], { toString: () => "true" }])("should reject non-primitive boolean input %#", (input) => {
      expectErr(coerce.bool(input), "Cannot coerce value to boolean");
    });

    it.each([() => "1", function toStringLike() {}])("should reject function input %#", (input) => {
      expectErr(coerce.int(input), "Cannot coerce value to integer");
      expectErr(coerce.number(input), "Cannot coerce value to number");
      expectErr(coerce.bool(input), "Cannot coerce value to boolean");
    });

    it.each([["value"], { toString: () => "value" }, () => "value"])(
      "should reject object and function input for strings %#",
      (input) => {
        expectErr(coerce.string(input), "Cannot coerce value to string");
      },
    );
  });

  describe("string.matches", () => {
    it("should be deterministic for global regular expressions", () => {
      const pattern = /^abc$/g;

      expect(val.string("abc").matches(pattern)).toEqual({ ok: true, value: "abc" });
      expect(val.string("abc").matches(pattern)).toEqual({ ok: true, value: "abc" });
    });
  });

  describe("string.fileType", () => {
    it("should validate file-like names by their type token", () => {
      expect(val.string("README").fileType(["readme"])).toEqual({ ok: true, value: "readme" });
      expect(val.string("avatar.PNG").fileType(["png"])).toEqual({ ok: true, value: "png" });
    });
  });

  describe("string length validators", () => {
    it.each([
      ["minLength", NaN, "Minimum length must be a finite non-negative number"],
      ["minLength", Infinity, "Minimum length must be a finite non-negative number"],
      ["minLength", -1, "Minimum length must be a finite non-negative number"],
      ["maxLength", NaN, "Maximum length must be a finite non-negative number"],
      ["maxLength", Infinity, "Maximum length must be a finite non-negative number"],
      ["maxLength", -1, "Maximum length must be a finite non-negative number"],
    ] as const)("should reject invalid %s limit %s", (validatorName, limit, error) => {
      expectErr(val.string("abc")[validatorName](limit), error);
    });
  });

  describe("string.url", () => {
    it.each(["example", "localhost", "0x10", "01"])("should reject weak host input %s", (input) => {
      expectErr(val.string(input).url(), "Invalid URL");
    });

    it("should keep valid public URLs normalized", () => {
      expect(val.string("example.com").url()).toEqual({ ok: true, value: "https://example.com/" });
    });

    it("should reject HTTP URLs when HTTPS is required", () => {
      expectErr(val.string("http://example.com").url({ forceHttps: true }), "Invalid URL");
    });

    it.each(["https://user@example.com", "https://user:pass@example.com"])(
      "should reject URLs with credentials %s",
      (input) => {
        expectErr(val.string(input).url(), "Invalid URL");
      },
    );
  });

  describe("string.email", () => {
    it.each(["user@example..com", "user@-example.com", "user@example-.com", "user@example.c", "user@b+c.com"])(
      "should reject invalid email host %s",
      (input) => {
        expectErr(val.string(input).email(), "Invalid email address");
      },
    );

    it("should keep normalized email addresses", () => {
      expect(val.string(" User+Label@Example.COM ").email()).toEqual({ ok: true, value: "User+Label@example.com" });
    });

    it.each([
      ["local", `${"a".repeat(65)}@example.com`],
      ["total", `${"a".repeat(245)}@example.com`],
    ])("should reject email addresses over the %s length limit", (_, input) => {
      expectErr(val.string(input).email(), "Invalid email address");
    });

    it.each([".user@example.com", "user.@example.com", "user..name@example.com", "a,b@example.com"])(
      "should reject invalid email local part %s",
      (input) => {
        expectErr(val.string(input).email(), "Invalid email address");
      },
    );
  });

  describe("custom", () => {
    it("should convert thrown errors to failed results", () => {
      expect(
        custom("value", () => {
          throw new Error("Nope");
        }),
      ).toMatchObject({ ok: false, error: expect.objectContaining({ message: "Nope" }) });
    });

    it("should convert thrown non-errors to failed results", () => {
      expect(
        custom("value", () => {
          throw "Nope";
        }),
      ).toMatchObject({ ok: false, error: expect.objectContaining({ message: "Nope" }) });
    });
  });

  describe("number validators", () => {
    it.each(["positive", "negative", "nonZero", "range"] as const)("should reject Infinity for %s", (validatorName) => {
      const validators = val.number(Infinity);
      const result =
        validatorName === "range" ? validators[validatorName]({ min: 0, max: 10 }) : validators[validatorName]();

      expectErr(result, "Must be a finite number");
    });

    it.each([
      [{ min: NaN }, "Range minimum must be a finite number"],
      [{ max: NaN }, "Range maximum must be a finite number"],
      [{ min: 10, max: 0 }, "Range minimum cannot be greater than maximum"],
    ] as const)("should reject invalid range options %#", (opts, error) => {
      expectErr(val.number(5).range(opts), error);
    });
  });
});
