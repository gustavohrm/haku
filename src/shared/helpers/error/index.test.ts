import { describe, expect, it } from "vitest";
import AppError, { DEFAULT_APP_ERROR_MESSAGE } from "./index";
import * as appErrorModule from "./index";

describe("AppError public surface", () => {
  it("should expose AppError as the only resolver", () => {
    expect(Object.keys(appErrorModule).sort()).toEqual(["AppError", "DEFAULT_APP_ERROR_MESSAGE", "default"]);
  });
});

describe("AppError", () => {
  it("should resolve known Supabase auth errors by exact code", () => {
    const originalError = {
      code: "invalid_credentials",
      message: "Invalid login credentials",
      name: "AuthApiError",
      status: 400,
    };

    const appError = new AppError(originalError);

    expect(appError.type).toBe("known");
    expect(appError.message).toBe("Invalid email or password.");
    expect(appError.messageKey).toBe("error.auth.invalidCredentials");
    expect(appError.source).toBe("supabase");
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it.each([
    ["email_not_confirmed", "Please confirm your email before signing in.", "error.auth.emailNotConfirmed"],
    ["otp_expired", "The link has expired. Request a new email.", "error.auth.otpExpired"],
    ["weak_password", "The password does not meet the security requirements.", "error.auth.weakPassword"],
  ])("should resolve common Supabase auth code %s", (code, message, messageKey) => {
    const originalError = {
      code,
      name: "AuthApiError",
    };

    const appError = new AppError(originalError);

    expect(appError.type).toBe("known");
    expect(appError.message).toBe(message);
    expect(appError.messageKey).toBe(messageKey);
    expect(appError.source).toBe("supabase");
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it("should resolve nested allowlisted wrapper errors when the surface error is unknown", () => {
    const nestedError = {
      code: "invalid_credentials",
      message: "Invalid login credentials",
      name: "AuthApiError",
    };
    const originalError = {
      cause: nestedError,
      message: "Outer wrapper without a match",
    };

    const appError = new AppError(originalError);

    expect(appError.type).toBe("known");
    expect(appError.message).toBe("Invalid email or password.");
    expect(appError.messageKey).toBe("error.auth.invalidCredentials");
    expect(appError.source).toBe("supabase");
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it.each(["originalError", "error"] as const)(
    "should resolve nested %s wrapper errors when the surface error is unknown",
    (fieldName) => {
      const nestedError = {
        code: "invalid_credentials",
        message: "Invalid login credentials",
        name: "AuthApiError",
      };
      const originalError = {
        [fieldName]: nestedError,
        message: "Outer wrapper without a match",
      };

      const appError = new AppError(originalError);

      expect(appError.type).toBe("known");
      expect(appError.message).toBe("Invalid email or password.");
      expect(appError.messageKey).toBe("error.auth.invalidCredentials");
      expect(appError.source).toBe("supabase");
      expect(appError.originalError).toBe(originalError);
      expect(appError.retryable).toBe(false);
    },
  );

  it("should resolve wrapper errors recursively up to depth three", () => {
    const nestedError = {
      code: "invalid_credentials",
      message: "Invalid login credentials",
      name: "AuthApiError",
    };
    const originalError = {
      cause: {
        originalError: {
          error: nestedError,
        },
      },
      message: "Outer wrapper without a match",
    };

    const appError = new AppError(originalError);

    expect(appError.type).toBe("known");
    expect(appError.message).toBe("Invalid email or password.");
    expect(appError.messageKey).toBe("error.auth.invalidCredentials");
    expect(appError.source).toBe("supabase");
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it("should stop unwrapping wrapper errors after depth three", () => {
    const nestedError = {
      code: "invalid_credentials",
      message: "Invalid login credentials",
      name: "AuthApiError",
    };
    const originalError = {
      cause: {
        originalError: {
          error: {
            cause: nestedError,
          },
        },
      },
      message: "Outer wrapper without a match",
    };

    const appError = new AppError(originalError);

    expect(appError.type).toBe("unknown");
    expect(appError.message).toBe(DEFAULT_APP_ERROR_MESSAGE);
    expect(appError.messageKey).toBe(null);
    expect(appError.source).toBe(null);
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it("should avoid cycles while unwrapping wrapper errors", () => {
    const nestedError = {
      code: "invalid_credentials",
      message: "Invalid login credentials",
      name: "AuthApiError",
    };
    const originalError: Record<string, unknown> = {
      message: "Outer wrapper without a match",
    };
    const wrappedError = {
      error: nestedError,
      originalError,
    };
    originalError.cause = wrappedError;

    const appError = new AppError(originalError);

    expect(appError.type).toBe("known");
    expect(appError.message).toBe("Invalid email or password.");
    expect(appError.messageKey).toBe("error.auth.invalidCredentials");
    expect(appError.source).toBe("supabase");
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it("should keep the surface classification before checking nested wrapper errors", () => {
    const originalError = {
      cause: {
        code: "invalid_credentials",
      },
      message: "Upload failed: Network error",
    };

    const appError = new AppError(originalError);

    expect(appError.type).toBe("known");
    expect(appError.message).toBe("Could not upload the file.");
    expect(appError.messageKey).toBe("error.storage.uploadFailed");
    expect(appError.source).toBe("supabase");
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it("should resolve known messages after deterministic punctuation normalization", () => {
    const originalError = new Error("Password update failed: New password should be different from the old password!");

    const appError = new AppError(originalError);

    expect(appError.type).toBe("known");
    expect(appError.message).toBe("The new password must be different from the current one.");
    expect(appError.messageKey).toBe("error.auth.samePassword");
    expect(appError.source).toBe("supabase");
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it("should resolve known browser errors by exact name", () => {
    const originalError = new DOMException("The quota has been exceeded.", "QuotaExceededError");

    const appError = new AppError(originalError);

    expect(appError.type).toBe("known");
    expect(appError.message).toBe("Could not save data in the browser.");
    expect(appError.messageKey).toBe("error.browser.quotaExceeded");
    expect(appError.source).toBe("browser");
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it("should resolve Supabase storage messages before app-defined prefixes", () => {
    const originalError = new Error("Upload failed: The resource already exists");

    const appError = new AppError(originalError);

    expect(appError.type).toBe("known");
    expect(appError.message).toBe("This file already exists.");
    expect(appError.messageKey).toBe("error.storage.alreadyExists");
    expect(appError.source).toBe("supabase");
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it("should resolve common Supabase storage not found messages", () => {
    const originalError = new Error("Failed to download file: The resource was not found");

    const appError = new AppError(originalError);

    expect(appError.type).toBe("known");
    expect(appError.message).toBe("File not found.");
    expect(appError.messageKey).toBe("error.storage.notFound");
    expect(appError.source).toBe("supabase");
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it("should resolve global unexpected error patterns more than once", () => {
    const firstOriginalError = new Error("Failed to fetch");
    const secondOriginalError = new Error("Failed to fetch");

    const firstError = new AppError(firstOriginalError);
    const secondError = new AppError(secondOriginalError);

    expect(firstError.type).toBe("unexpected");
    expect(firstError.message).toBe("Could not connect to the service.");
    expect(firstError.messageKey).toBe("error.network.connectionFailed");
    expect(firstError.source).toBe("browser");
    expect(firstError.originalError).toBe(firstOriginalError);
    expect(firstError.retryable).toBe(true);

    expect(secondError.type).toBe("unexpected");
    expect(secondError.message).toBe("Could not connect to the service.");
    expect(secondError.messageKey).toBe("error.network.connectionFailed");
    expect(secondError.source).toBe("browser");
    expect(secondError.originalError).toBe(secondOriginalError);
    expect(secondError.retryable).toBe(true);
  });

  it("should keep the original error and use the provided fallback message for unknown errors", () => {
    const originalError = new Error("Unexpected failure");

    const appError = new AppError(originalError, { fallbackMessage: "Something went wrong." });

    expect(appError.type).toBe("unknown");
    expect(appError.message).toBe("Something went wrong.");
    expect(appError.messageKey).toBe(null);
    expect(appError.source).toBe(null);
    expect(appError.originalError).toBe(originalError);
    expect(appError.retryable).toBe(false);
  });

  it("should use the default fallback message when none is provided", () => {
    const appError = new AppError(500);

    expect(appError.type).toBe("unknown");
    expect(appError.message).toBe(DEFAULT_APP_ERROR_MESSAGE);
    expect(appError.messageKey).toBe(null);
    expect(appError.source).toBe(null);
    expect(appError.originalError).toBe(500);
    expect(appError.retryable).toBe(false);
  });

  it("should extend Error so other modules can treat it like an error", () => {
    const appError = new AppError(new Error("unexpected"));

    expect(appError).toBeInstanceOf(Error);
    expect(appError).toBeInstanceOf(AppError);
  });

  it("should preserve the original resolved fields when wrapped again", () => {
    const firstError = new AppError(new Error("Upload failed: Network error"));
    const secondError = new AppError(firstError, { fallbackMessage: "Should not replace." });

    expect(secondError.type).toBe(firstError.type);
    expect(secondError.message).toBe(firstError.message);
    expect(secondError.messageKey).toBe(firstError.messageKey);
    expect(secondError.source).toBe(firstError.source);
    expect(secondError.originalError).toBe(firstError.originalError);
    expect(secondError.retryable).toBe(firstError.retryable);
  });

  it("should mark network errors as retryable", () => {
    expect(new AppError(new Error("Failed to fetch")).retryable).toBe(true);
    expect(new AppError(new Error("Load failed")).retryable).toBe(true);
    expect(new AppError(new Error("NetworkError occurred")).retryable).toBe(true);
  });

  it("should mark timeout errors as retryable", () => {
    expect(new AppError(new Error("Request timed out")).retryable).toBe(true);
    expect(new AppError(new Error("The operation has timed out")).retryable).toBe(true);
  });

  it("should not mark abort errors as retryable", () => {
    expect(new AppError(new Error("The operation was aborted")).retryable).toBe(false);
  });
});
