export const PLUGIN_NAME = "vite-plugin-icons";

export const ICON_MARKER_PREFIX = "ic-";

export const TRANSFORM_EXTENSIONS = /\.(html|js|ts|jsx|tsx)$/;

/**
 * Matches <i> tags with a class attribute (self-closing or explicit).
 *
 * Groups: 1 = attrs before class, 2 = quote char, 3 = class string,
 *         4 = attrs after class
 */
export const ICON_TAG_REGEX = /<i\b([^>]*?)class=(["'])([^"']*?)\2([^>]*?)(?:>\s*<\/i>|\s*\/?>)/gi;
