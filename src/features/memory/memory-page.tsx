import { InternalPage } from "@app/internal-page";
import {
  events,
  type Loss,
  type LossSignal,
  type MemoryReport,
  type Position,
  type Pressure,
  type ProcessKind,
  type SlotMemory,
  type Tab,
} from "@bindings";
import { Favicon } from "@features/bookmarks/favicon";
import { reportUnexpected } from "@features/feedback/feedback";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { unwrap } from "@ipc/result";
import { cx } from "@shared/class-names";
import { t, type TranslationKey } from "@shared/i18n";
import { useEffect, useState, type ReactNode } from "react";

import { formatBytes, formatShare, sumKnown } from "./format";

const PRESSURE_LABELS: Record<Pressure, TranslationKey> = {
  normal: "memory.pressure.normal",
  tight: "memory.pressure.tight",
  critical: "memory.pressure.critical",
};

const LOSS_LABELS: Record<Loss, TranslationKey> = {
  none: "memory.loss.none",
  state: "memory.loss.state",
  work: "memory.loss.work",
};

const POSITION_LABELS: Record<Position, TranslationKey> = {
  visible: "memory.position.visible",
  mustRun: "memory.position.mustRun",
  kept: "memory.position.kept",
};

const SIGNAL_LABELS: Record<LossSignal, TranslationKey> = {
  keptSite: "memory.signal.keptSite",
  unsaved: "memory.signal.unsaved",
  unloadArmed: "memory.signal.unloadArmed",
  formResult: "memory.signal.formResult",
  interactions: "memory.signal.interactions",
  mediaPaused: "memory.signal.mediaPaused",
  unreadable: "memory.signal.unreadable",
};

const PROCESS_LABELS: Record<ProcessKind, TranslationKey> = {
  browser: "memory.process.browser",
  renderer: "memory.process.renderer",
  gpu: "memory.process.gpu",
  utility: "memory.process.utility",
  other: "memory.process.other",
};

/**
 * What each webview slot holds and how short of memory the machine is.
 *
 * It exists so the optimization constants can be tuned against real pages,
 * and so a user can see why a tab was or was not kept. Rust sends a fresh
 * report on every tick.
 */
export function MemoryPage() {
  const report = useMemoryReport();
  const { tabs, private: isPrivate } = useBrowserState();
  const slotsTotal = sumKnown(report?.slots.map((slot) => slot.bytes) ?? []);
  const engineTotal = sumKnown(report?.unattributed.map((process) => process.bytes) ?? []);
  const largest = Math.max(
    0,
    ...(report?.slots.map((slot) => slot.bytes ?? 0) ?? []),
    ...(report?.unattributed.map((process) => process.bytes) ?? []),
  );

  return (
    <InternalPage title={t("memory.title")}>
      {report && (
        <>
          <Summary report={report} />

          <Section title={t("memory.slots")} total={formatBytes(slotsTotal)}>
            {report.slots.map((slot) => (
              <SlotRow
                key={slot.slot}
                slot={slot}
                tab={tabs.find((candidate) => candidate.id === slot.tab)}
                isPrivate={isPrivate}
                share={shareOf(slot.bytes, largest)}
              />
            ))}
          </Section>

          <Section title={t("memory.unattributed")} total={formatBytes(engineTotal)}>
            {report.unattributed.map((process) => (
              <li key={process.pid} className={ROW}>
                <span className="min-w-0 flex-1 truncate">
                  {t(PROCESS_LABELS[process.kind])}
                  <span className="text-text-secondary ms-2 text-xs tabular-nums">
                    {t("memory.pid")} {process.pid}
                  </span>
                </span>
                <Amount bytes={process.bytes} share={shareOf(process.bytes, largest)} />
              </li>
            ))}
          </Section>
        </>
      )}
    </InternalPage>
  );
}

const ROW = "flex min-h-11 items-center gap-3 px-4 py-1.5";

/** How much of the widest bar a row's memory fills. */
function shareOf(bytes: number | null, largest: number): number {
  return bytes === null || largest === 0 ? 0 : bytes / largest;
}

const PRESSURE_DOT: Record<Pressure, string> = {
  normal: "bg-success",
  tight: "bg-warning",
  critical: "bg-danger",
};

/** The three figures that answer whether memory is a problem right now. */
function Summary({ report }: { report: MemoryReport }) {
  // Kept memory can exceed a budget of nothing, and the meter then shows it full.
  const used = report.budget === 0 ? (report.kept > 0 ? 1 : 0) : report.kept / report.budget;
  return (
    <dl className="flex flex-wrap gap-x-10 gap-y-5 px-1">
      <Figure label={t("memory.pressure")}>
        <span className="flex items-center gap-2">
          <span className={cx("size-2 rounded-full", PRESSURE_DOT[report.pressure])} aria-hidden="true" />
          {t(PRESSURE_LABELS[report.pressure])}
        </span>
      </Figure>
      <Figure label={t("memory.headroom")}>{formatShare(report.headroom)}</Figure>
      <Figure label={t("memory.budget")} grow>
        <span className="flex items-baseline gap-1.5">
          {formatBytes(report.kept)}
          <span className="text-text-secondary text-sm font-normal">/ {formatBytes(report.budget)}</span>
        </span>
        <Meter value={used} className="mt-2 h-1.5 max-w-64" />
      </Figure>
    </dl>
  );
}

function Figure({ label, grow = false, children }: { label: string; grow?: boolean; children: ReactNode }) {
  return (
    <div className={cx("flex min-w-28 flex-col gap-1", grow && "flex-1")}>
      <dt className="text-text-secondary text-sm">{label}</dt>
      <dd className="text-lg font-semibold tabular-nums">{children}</dd>
    </div>
  );
}

/** A bar that fills from the start. Its width eases, so a tick reads as a change and not a jump. */
function Meter({ value, className }: { value: number; className?: string }) {
  return (
    <span className={cx("bg-text/8 block overflow-hidden rounded-full", className)} aria-hidden="true">
      <span
        className="bg-text/55 block h-full origin-left rounded-full transition-transform duration-300 ease-out motion-reduce:transition-none"
        style={{ transform: `scaleX(${Math.max(0, Math.min(1, value))})` }}
      />
    </span>
  );
}

function Section({ title, total, children }: { title: string; total: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-text-secondary flex items-baseline justify-between px-1 text-sm font-medium">
        {title}
        <span className="tabular-nums">{total}</span>
      </h2>
      <ul className="card soft edgeless flex flex-col py-1 [--surface-padding:0]">{children}</ul>
    </section>
  );
}

/** A row's memory, as a figure and as a bar against the largest row on the page. */
function Amount({ bytes, share }: { bytes: number | null; share: number }) {
  return (
    <>
      <Meter value={share} className="h-1 w-24 shrink-0" />
      <span className="w-16 shrink-0 text-right tabular-nums">{formatBytes(bytes)}</span>
    </>
  );
}

interface SlotRowProps {
  slot: SlotMemory;
  /** The slot's tab, when it is one of this window's. */
  tab: Tab | undefined;
  isPrivate: boolean;
  share: number;
}

function SlotRow({ slot, tab, isPrivate, share }: SlotRowProps) {
  const visit = tab?.history.entries[tab.history.index];
  const facts = [t(stateLabel(tab)), slot.position ? t(POSITION_LABELS[slot.position]) : null, lossLabel(slot)].filter(
    (fact): fact is string => fact !== null,
  );
  return (
    <li className={ROW} data-slot={slot.slot}>
      <span className="text-text-secondary w-4 shrink-0 text-center text-xs tabular-nums" title={t("memory.slot")}>
        {slot.slot}
      </span>
      {tab ? (
        <>
          {/* The generic icon in a private window: fetching the site's would send the ordinary profile's cookies. */}
          <Favicon key={visit?.url ?? ""} url={isPrivate ? null : (visit?.favicon ?? null)} />
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate">{tabLabel(tab)}</span>
            <span className="text-text-secondary truncate text-xs" title={facts.join(" · ")}>
              {facts.join(" · ")}
            </span>
          </span>
        </>
      ) : (
        // The pool is shared by every window, so a slot can hold a tab this one does not list.
        <span className="text-text-secondary min-w-0 flex-1">
          {t(slot.tab === null ? "memory.slot.free" : "memory.slot.otherWindow")}
        </span>
      )}
      <Amount bytes={slot.bytes} share={share} />
    </li>
  );
}

/** The latest report, loaded once and then replaced on every tick. */
function useMemoryReport(): MemoryReport | null {
  const [report, setReport] = useState<MemoryReport | null>(null);

  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;

    const connect = async () => {
      // Listening first, so a tick landing during the first read is not lost.
      const stop = await events.memoryChanged.listen((event) => {
        if (isMounted) {
          setReport(event.payload);
        }
      });
      if (!isMounted) {
        stop();
        return;
      }
      unlisten = stop;
      let first: MemoryReport;
      try {
        first = await unwrap(commands.memoryReport());
      } catch (error) {
        reportUnexpected(error);
        return;
      }
      if (isMounted) {
        setReport((current) => current ?? first);
      }
    };
    void connect();

    return () => {
      isMounted = false;
      unlisten?.();
    };
  }, []);

  return report;
}

function tabLabel(tab: Tab): string {
  const visit = tab.history.entries[tab.history.index];
  return visit?.title.trim() || visit?.url || t("tabs.untitled");
}

/** The loss level, followed by the signals behind it. */
function lossLabel(slot: SlotMemory): string | null {
  if (slot.loss === null) {
    return null;
  }
  const level = t(LOSS_LABELS[slot.loss]);
  const reasons = slot.signals.map((signal) => t(SIGNAL_LABELS[signal]));
  return reasons.length === 0 ? level : `${level}: ${reasons.join(", ")}`;
}

function stateLabel(tab: Tab | undefined): TranslationKey {
  switch (tab?.presence.status) {
    case "live":
      return "memory.state.live";
    case "frozen":
      return "memory.state.frozen";
    default:
      return "memory.state.parked";
  }
}
