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
      // A named group, so a control can show actions on the row's hover or
      // focus without answering to a list's own `group` inside it.
      className={cx(
        "group/setting flex gap-x-6 gap-y-2",
        row.dense
          ? "hover:bg-text/4 focus-within:bg-text/4 mx-1 min-h-9 rounded-(--radius-control) px-3 py-1"
          : "px-4 py-3",
        // A control that does not fit beside the title, as in a narrow split
        // pane, wraps under it rather than running off the page.
        row.wide ? "flex-col" : "flex-wrap items-center justify-between",
      )}
      data-setting={row.id}
    >
      <Text beside={!row.wide}>
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
      <div className={cx("min-w-0", row.wide ? "w-full" : "max-w-full shrink-0")}>
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

/**
 * The well a list of entries sits in, inside a row: the page's colour, so the
 * list has an edge of its own on the plate and its rows line up with nothing
 * outside it.
 */
export const LIST_WELL = "bg-background flex flex-col rounded-(--radius-control) p-1";

/** What the well says while its list has nothing in it. */
export const EMPTY_WELL = "bg-background text-text-secondary rounded-(--radius-control) px-3 py-2 text-sm";

/** The title and description; `beside` a control, it keeps 16 rem before the control wraps under it. */
function Text({ beside, children }: { beside: boolean; children: ReactNode }) {
  return <div className={cx("flex max-w-xl min-w-0 flex-col gap-0.5", beside && "flex-[1_1_16rem]")}>{children}</div>;
}
