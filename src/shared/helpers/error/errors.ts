export type ErrorFeedbackSource = "browser" | "internal" | "supabase";

export interface ErrorFeedback {
  feedback: string;
  feedbackKey: string;
  source: ErrorFeedbackSource;
  retryable?: boolean;
}

export interface KnownErrorPrefixDefinition extends ErrorFeedback {
  prefix: string;
}

export interface UnexpectedErrorDefinition extends ErrorFeedback {
  pattern: RegExp;
}

export const KNOWN_ERROR_FEEDBACK_BY_CODE: Readonly<Record<string, ErrorFeedback>> = {
  invalid_credentials: {
    feedback: "Invalid email or password.",
    feedbackKey: "error.auth.invalidCredentials",
    source: "supabase",
  },
  email_not_confirmed: {
    feedback: "Please confirm your email before signing in.",
    feedbackKey: "error.auth.emailNotConfirmed",
    source: "supabase",
  },
  over_email_send_rate_limit: {
    feedback: "Please wait a moment before trying again.",
    feedbackKey: "error.auth.rateLimitExceeded",
    source: "supabase",
  },
  otp_expired: {
    feedback: "The link has expired. Request a new email.",
    feedbackKey: "error.auth.otpExpired",
    source: "supabase",
  },
  same_password: {
    feedback: "The new password must be different from the current one.",
    feedbackKey: "error.auth.samePassword",
    source: "supabase",
  },
  weak_password: {
    feedback: "The password does not meet the security requirements.",
    feedbackKey: "error.auth.weakPassword",
    source: "supabase",
  },
};

export const KNOWN_ERROR_FEEDBACK_BY_NAME: Readonly<Record<string, ErrorFeedback>> = {
  QuotaExceededError: {
    feedback: "Could not save data in the browser.",
    feedbackKey: "error.browser.quotaExceeded",
    source: "browser",
  },
  SecurityError: {
    feedback: "The browser blocked access to storage.",
    feedbackKey: "error.browser.securityBlocked",
    source: "browser",
  },
};

export const KNOWN_ERROR_FEEDBACK_BY_MESSAGE: Readonly<Record<string, ErrorFeedback>> = {
  "Authentication failed: Invalid login credentials": {
    feedback: "Invalid email or password.",
    feedbackKey: "error.auth.invalidCredentials",
    source: "supabase",
  },
  "Password update failed: New password should be different from the old password": {
    feedback: "The new password must be different from the current one.",
    feedbackKey: "error.auth.samePassword",
    source: "supabase",
  },
  "Invalid storage path": {
    feedback: "Invalid storage path.",
    feedbackKey: "error.storage.invalidPath",
    source: "internal",
  },
  "Upload failed: The resource already exists": {
    feedback: "This file already exists.",
    feedbackKey: "error.storage.alreadyExists",
    source: "supabase",
  },
  "Failed to download file: The resource was not found": {
    feedback: "File not found.",
    feedbackKey: "error.storage.notFound",
    source: "supabase",
  },
  "Failed to delete file: The resource was not found": {
    feedback: "File not found.",
    feedbackKey: "error.storage.notFound",
    source: "supabase",
  },
  "No files to compress": {
    feedback: "No files to compress.",
    feedbackKey: "error.storage.noFilesToCompress",
    source: "internal",
  },
};

export const KNOWN_ERROR_PREFIX_DEFINITIONS: readonly KnownErrorPrefixDefinition[] = [
  {
    prefix: "Authentication failed:",
    feedback: "Could not sign in.",
    feedbackKey: "error.auth.authFailed",
    source: "supabase",
  },
  {
    prefix: "Password reset failed:",
    feedback: "Could not send the password reset email.",
    feedbackKey: "error.auth.passwordResetFailed",
    source: "supabase",
  },
  {
    prefix: "Password update failed:",
    feedback: "Could not reset the password.",
    feedbackKey: "error.auth.passwordUpdateFailed",
    source: "supabase",
  },
  {
    prefix: "Logout failed:",
    feedback: "Could not sign out.",
    feedbackKey: "error.auth.logoutFailed",
    source: "supabase",
  },
  {
    prefix: "Upload failed:",
    feedback: "Could not upload the file.",
    feedbackKey: "error.storage.uploadFailed",
    source: "supabase",
  },
  {
    prefix: "Failed to download file:",
    feedback: "Could not download the file.",
    feedbackKey: "error.storage.downloadFailed",
    source: "supabase",
  },
  {
    prefix: "Download failed:",
    feedback: "Could not download the file.",
    feedbackKey: "error.storage.downloadFailed",
    source: "internal",
  },
  {
    prefix: "Failed to delete file:",
    feedback: "Could not delete the file.",
    feedbackKey: "error.storage.deleteFailed",
    source: "supabase",
  },
  {
    prefix: "Failed to list files:",
    feedback: "Could not load the files.",
    feedbackKey: "error.storage.listFailed",
    source: "supabase",
  },
];

export const UNEXPECTED_ERROR_DEFINITIONS: readonly UnexpectedErrorDefinition[] = [
  {
    pattern: /\b(failed to fetch|load failed|network ?error)\b/i,
    feedback: "Could not connect to the service.",
    feedbackKey: "error.network.connectionFailed",
    source: "browser",
    retryable: true,
  },
  {
    pattern: /\b(aborterror|aborted|operation was aborted)\b/i,
    feedback: "The operation was cancelled.",
    feedbackKey: "error.network.aborted",
    source: "browser",
  },
  {
    pattern: /\b(timeout|timed out)\b/i,
    feedback: "The operation took too long to respond.",
    feedbackKey: "error.network.timeout",
    source: "browser",
    retryable: true,
  },
];
