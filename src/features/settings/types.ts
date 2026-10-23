import { type Settings } from "@bindings";
import { type TranslationKey } from "@shared/i18n";
import { type ComponentType } from "react";

/**
 * Writes a change through to Rust.
 *
 * @param change - A patch to merge into the settings, or a function computing the whole
 * new settings from the current ones. Prefer the function when the new value depends on
 * the old one, so that it reads the value as it is when the change is made.
 * @returns What Rust stored, which may differ from what was sent.
 */
export type UpdateSettings = (
  change: Partial<Settings> | ((current: Settings) => Settings),
) => Promise<Settings | null>;

export interface SettingContext {
  settings: Settings;
  update: UpdateSettings;
}

/** What a row hands its control, besides the settings. */
export interface ControlProps extends SettingContext {
  /** For the control the row's title labels. */
  id: string;
  /** The title's id, for a control the title cannot label directly, such as a list. */
  labelId: string;
  describedBy: string | undefined;
  disabled: boolean;
}

export interface SettingRowDefinition {
  /** Stable, for tests and for linking to a row later. */
  id: string;
  title: TranslationKey;
  description?: TranslationKey;
  /** Comma-separated words a search should also find the row by, such as "dark mode". */
  keywords?: TranslationKey;
  /** Why the row has no effect in the current state, or `null` while it does. */
  disabledReason?: (settings: Settings) => TranslationKey | null;
  /** The control takes the row's width under the title rather than its right side. */
  wide?: boolean;
  /** `false` for a control the title cannot label, such as a list; it gets `labelId` instead. */
  labelsControl?: false;
  Control: ComponentType<ControlProps>;
}

export interface SettingGroup {
  title?: TranslationKey;
  rows: SettingRowDefinition[];
}

export interface SettingSection {
  /** The path after `haku://settings/`. */
  id: string;
  /** An icon class, written out in full so the icon plugin generates it. */
  icon: string;
  title: TranslationKey;
  groups: SettingGroup[];
}
