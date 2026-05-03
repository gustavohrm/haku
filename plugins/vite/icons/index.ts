import type { Plugin } from "vite";

import { icons } from "./data";
import { ICON_MARKER_PREFIX, ICON_TAG_REGEX, PLUGIN_NAME, TRANSFORM_EXTENSIONS } from "./constants";

type IconName = keyof typeof icons;
interface ResolvedIcon {
  iconClass: string;
  iconName: IconName;
}

const ICON_NAME_SET = new Set<IconName>(Object.keys(icons) as IconName[]);

function splitClasses(classValue: string): string[] {
  return classValue.split(/\s+/).filter(Boolean);
}

function isIconName(value: string): value is IconName {
  return ICON_NAME_SET.has(value as IconName);
}

function resolveIcon(classValue: string): ResolvedIcon | null {
  const classes = splitClasses(classValue);

  for (const className of classes) {
    if (!className.startsWith(ICON_MARKER_PREFIX)) continue;

    const iconNameCandidate = className.slice(ICON_MARKER_PREFIX.length);
    if (!isIconName(iconNameCandidate)) continue;

    return { iconClass: className, iconName: iconNameCandidate };
  }

  return null;
}

function stripIconClass(classValue: string, iconClass: string): string | null {
  const remaining = splitClasses(classValue)
    .filter((className) => className !== iconClass)
    .join(" ");
  return remaining || null;
}

function joinPassthroughAttributes(attrsBeforeClass: string, attrsAfterClass: string): string {
  const segments = [attrsBeforeClass.trim(), attrsAfterClass.trim()].filter(Boolean);
  return segments.length > 0 ? ` ${segments.join(" ")}` : "";
}

function createIconTagRegex(): RegExp {
  return new RegExp(ICON_TAG_REGEX.source, ICON_TAG_REGEX.flags);
}

function buildSvgReplacement(
  match: string,
  attrsBeforeClass: string,
  classValue: string,
  attrsAfterClass: string,
): string {
  const icon = resolveIcon(classValue);
  if (!icon) return match;

  const extraClasses = stripIconClass(classValue, icon.iconClass);
  const classAttr = extraClasses ? ` class="${extraClasses}"` : "";
  const passthroughAttrStr = joinPassthroughAttributes(attrsBeforeClass, attrsAfterClass);

  return icons[icon.iconName].replace(/^<svg\b/i, `<svg${classAttr}${passthroughAttrStr}`);
}

function replaceIconTags(source: string): string {
  const iconTagRegex = createIconTagRegex();

  return source.replace(iconTagRegex, (match, before: string, _quote: string, classValue: string, after: string) =>
    buildSvgReplacement(match, before, classValue, after),
  );
}

export default function iconsPlugin(): Plugin {
  return {
    name: PLUGIN_NAME,
    enforce: "pre",

    transformIndexHtml: {
      order: "pre",
      handler: (html: string) => replaceIconTags(html),
    },

    transform(code: string, id: string) {
      const fileId = id.split("?", 1)[0];
      if (!TRANSFORM_EXTENSIONS.test(fileId)) return null;
      if (!code.includes(ICON_MARKER_PREFIX)) return null;

      const transformed = replaceIconTags(code);
      if (transformed === code) return null;

      return { code: transformed, map: null };
    },
  };
}
