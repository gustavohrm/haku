import {
  KNOWN_ERROR_FEEDBACK_BY_CODE,
  KNOWN_ERROR_FEEDBACK_BY_MESSAGE,
  KNOWN_ERROR_FEEDBACK_BY_NAME,
  KNOWN_ERROR_PREFIX_DEFINITIONS,
  UNEXPECTED_ERROR_DEFINITIONS,
  type ErrorFeedback,
  type ErrorFeedbackSource,
} from "./errors";

export interface AppErrorOptions {
  fallbackMessage?: string;
}

export type AppErrorType = "known" | "unexpected" | "unknown";
export type AppErrorSource = ErrorFeedbackSource | null;

interface NormalizedError {
  code: string | null;
  message: string | null;
  name: string | null;
}

interface ErrorClassification {
  type: Exclude<AppErrorType, "unknown">;
  message: string;
  messageKey: string;
  source: ErrorFeedbackSource;
  retryable: boolean;
}

interface AppErrorResolution {
  type: AppErrorType;
  message: string;
  messageKey: string | null;
  source: AppErrorSource;
  originalError: unknown;
  retryable: boolean;
}

export const DEFAULT_APP_ERROR_MESSAGE = "An unexpected error occurred.";

const ERROR_IDENTIFIER_TRAILING_PUNCTUATION_PATTERN = /[.!?]+$/;
const ERROR_UNWRAP_MAX_DEPTH = 3;
const ERROR_WRAPPER_FIELD_NAMES = ["cause", "originalError", "error"] as const;

const isRecord = (value: unknown): value is Record<string, unknown> => {
  return typeof value === "object" && value !== null;
};

const getStringField = (source: Record<string, unknown>, key: string): string | null => {
  const value = source[key];
  return typeof value === "string" ? value : null;
};

const normalizeErrorIdentifier = (identifier: string): string => {
  return identifier.trim().replace(ERROR_IDENTIFIER_TRAILING_PUNCTUATION_PATTERN, "");
};

const NORMALIZED_KNOWN_MESSAGES: ReadonlyMap<string, ErrorFeedback> = new Map(
  Object.entries(KNOWN_ERROR_FEEDBACK_BY_MESSAGE).map(([key, value]) => [normalizeErrorIdentifier(key), value]),
);

const normalizeError = (error: unknown): NormalizedError => {
  if (typeof error === "string") {
    return { code: null, message: error, name: null };
  }

  if (!isRecord(error)) {
    return { code: null, message: null, name: null };
  }

  return {
    code: getStringField(error, "code"),
    message: getStringField(error, "message"),
    name: getStringField(error, "name"),
  };
};

const getWrappedErrorCandidates = (error: unknown): unknown[] => {
  if (!isRecord(error)) {
    return [];
  }

  return ERROR_WRAPPER_FIELD_NAMES.map((fieldName) => error[fieldName]).filter(
    (value) => value !== undefined && value !== null,
  );
};

const getErrorCandidates = (error: unknown): unknown[] => {
  const candidates: unknown[] = [];
  const visitedObjects = new Set<object>();

  if (isRecord(error)) {
    visitedObjects.add(error);
  }

  const pendingCandidates = [{ depth: 0, value: error }];

  for (let index = 0; index < pendingCandidates.length; index += 1) {
    const candidate = pendingCandidates[index];

    candidates.push(candidate.value);

    if (candidate.depth >= ERROR_UNWRAP_MAX_DEPTH) {
      continue;
    }

    for (const wrappedErrorCandidate of getWrappedErrorCandidates(candidate.value)) {
      if (isRecord(wrappedErrorCandidate)) {
        if (visitedObjects.has(wrappedErrorCandidate)) {
          continue;
        }

        visitedObjects.add(wrappedErrorCandidate);
      }

      pendingCandidates.push({ depth: candidate.depth + 1, value: wrappedErrorCandidate });
    }
  }

  return candidates;
};

const getKnownMessageFeedback = (message: string): ErrorClassification | null => {
  const normalizedMessage = normalizeErrorIdentifier(message);

  const exactFeedback = NORMALIZED_KNOWN_MESSAGES.get(normalizedMessage);

  if (exactFeedback !== undefined) {
    return {
      type: "known",
      message: exactFeedback.feedback,
      messageKey: exactFeedback.feedbackKey,
      source: exactFeedback.source,
      retryable: exactFeedback.retryable ?? false,
    };
  }

  const prefixDefinition = KNOWN_ERROR_PREFIX_DEFINITIONS.find((definition) => {
    return normalizedMessage.startsWith(definition.prefix);
  });

  if (prefixDefinition === undefined) {
    return null;
  }

  return {
    type: "known",
    message: prefixDefinition.feedback,
    messageKey: prefixDefinition.feedbackKey,
    source: prefixDefinition.source,
    retryable: prefixDefinition.retryable ?? false,
  };
};

const resolveDeterministicKnownError = ({ code, message, name }: NormalizedError): ErrorClassification | null => {
  if (code !== null) {
    const feedback = KNOWN_ERROR_FEEDBACK_BY_CODE[code];

    if (feedback !== undefined) {
      return {
        type: "known",
        message: feedback.feedback,
        messageKey: feedback.feedbackKey,
        source: feedback.source,
        retryable: feedback.retryable ?? false,
      };
    }
  }

  if (name !== null) {
    const feedback = KNOWN_ERROR_FEEDBACK_BY_NAME[name];

    if (feedback !== undefined) {
      return {
        type: "known",
        message: feedback.feedback,
        messageKey: feedback.feedbackKey,
        source: feedback.source,
        retryable: feedback.retryable ?? false,
      };
    }
  }

  if (message !== null) {
    const feedback = getKnownMessageFeedback(message);

    if (feedback !== null) {
      return feedback;
    }
  }

  return null;
};

const resolveHeuristicUnexpectedError = ({ message }: NormalizedError): ErrorClassification | null => {
  if (message === null) {
    return null;
  }

  const definition = UNEXPECTED_ERROR_DEFINITIONS.find((currentDefinition) => {
    return currentDefinition.pattern.test(message);
  });

  if (definition === undefined) {
    return null;
  }

  return {
    type: "unexpected",
    message: definition.feedback,
    messageKey: definition.feedbackKey,
    source: definition.source,
    retryable: definition.retryable ?? false,
  };
};

const classifyNormalizedError = (normalizedError: NormalizedError): ErrorClassification | null => {
  const knownError = resolveDeterministicKnownError(normalizedError);

  if (knownError !== null) {
    return knownError;
  }

  return resolveHeuristicUnexpectedError(normalizedError);
};

export class AppError extends Error {
  readonly type: AppErrorType;
  declare readonly message: string;
  readonly messageKey: string | null;
  readonly source: AppErrorSource;
  readonly originalError: unknown;
  readonly retryable: boolean;

  constructor(error: unknown, options: AppErrorOptions = {}) {
    const resolved = AppError.resolve(error, options);

    super(resolved.message);

    Object.setPrototypeOf(this, new.target.prototype);

    this.name = "AppError";
    this.type = resolved.type;
    this.messageKey = resolved.messageKey;
    this.source = resolved.source;
    this.originalError = resolved.originalError;
    this.retryable = resolved.retryable;
  }

  private static resolve(error: unknown, options: AppErrorOptions): AppErrorResolution {
    if (error instanceof AppError) {
      return {
        type: error.type,
        message: error.message,
        messageKey: error.messageKey,
        source: error.source,
        originalError: error.originalError,
        retryable: error.retryable,
      };
    }

    const fallbackMessage = options.fallbackMessage ?? DEFAULT_APP_ERROR_MESSAGE;
    const errorCandidates = getErrorCandidates(error);

    for (const errorCandidate of errorCandidates) {
      const classification = classifyNormalizedError(normalizeError(errorCandidate));

      if (classification !== null) {
        return {
          type: classification.type,
          message: classification.message,
          messageKey: classification.messageKey,
          source: classification.source,
          originalError: error,
          retryable: classification.retryable,
        };
      }
    }

    return {
      type: "unknown",
      message: fallbackMessage,
      messageKey: null,
      source: null,
      originalError: error,
      retryable: false,
    };
  }
}

export default AppError;
