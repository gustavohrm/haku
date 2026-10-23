import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useId, type ReactNode } from "react";

import { type SettingContext, type SettingGroup, type SettingRowDefinition } from "./types";

interface SettingsGroupProps {
  group: SettingGroup;
  context: SettingContext;
}

/** One plate of rows, with its heading above it when it has one. */
export function SettingsGroup({ group, context }: SettingsGroupProps) {
  return (
    <GroupPlate title={group.title ? t(group.title) : undefined}>
      {group.rows.map((row) => (
        <SettingRow key={row.id} row={row} context={context} />
      ))}
    </GroupPlate>
  );
}

/**
 * The plate a group's rows sit on: a soft step up from the page, with no
 * border and no lines between rows. Spacing separates the rows; a line would
 * only add noise to what the plate already groups.
 */
export function GroupPlate({ title, children }: { title?: string | undefined; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      {title && <h2 className="text-text-secondary px-1 text-sm font-medium">{title}</h2>}
      <div className="card soft edgeless flex flex-col py-1 [--surface-padding:0]">{children}</div>
    </section>
  );
}

interface SettingRowProps {
  row: SettingRowDefinition;
  context: SettingContext;
}

/**
 * One setting: its title and description on the left, its control on the right.
 *
 * A wide control, such as a list or a URL, takes the row's width under the
 * title instead. A row that has no effect in the current state is disabled,
 * with the reason in place of its description, so nobody has to hover to find
 * out why it does not respond.
 */
export function SettingRow({ row, context }: SettingRowProps) {
  const controlId = useId();
  const titleId = useId();
  const descriptionId = useId();
  const reason = row.disabledReason?.(context.settings) ?? null;
  const description = reason ?? row.description;
  const { Control } = row;

  return (
    <div
      className={cx("flex gap-x-6 gap-y-2 px-4 py-3", row.wide ? "flex-col" : "items-center justify-between")}
      data-setting={row.id}
    >
      <Text>
        {/* A list has no single control to label, so it is labelled by its title instead. */}
        {row.labelsControl === false ? (
          <span id={titleId} className={cx(reason && "text-text-secondary")}>
            {t(row.title)}
          </span>
        ) : (
          <label id={titleId} htmlFor={controlId} className={cx(reason && "text-text-secondary")}>
            {t(row.title)}
          </label>
        )}
        {description && (
          <span id={descriptionId} className="text-text-secondary text-sm">
            {t(description)}
          </span>
        )}
      </Text>
      <div className={cx("min-w-0", row.wide ? "w-full" : "shrink-0")}>
        <Control
          {...context}
          id={controlId}
          labelId={titleId}
          describedBy={description ? descriptionId : undefined}
          disabled={reason !== null}
        />
      </div>
    </div>
  );
}

function Text({ children }: { children: ReactNode }) {
  return <div className="flex max-w-xl min-w-0 flex-col gap-0.5">{children}</div>;
}
