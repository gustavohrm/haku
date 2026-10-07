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
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { unwrap } from "@ipc/result";
import { t, type TranslationKey } from "@shared/i18n";
import { useEffect, useState } from "react";

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
  const { tabs } = useBrowserState();

  return (
    <InternalPage title={t("memory.title")}>
      {report && (
        <>
          <section className="card stack">
            <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1">
              <dt className="text-text-secondary">{t("memory.pressure")}</dt>
              <dd>{t(PRESSURE_LABELS[report.pressure])}</dd>
              <dt className="text-text-secondary">{t("memory.headroom")}</dt>
              <dd>{formatShare(report.headroom)}</dd>
              <dt className="text-text-secondary">{t("memory.budget")}</dt>
              <dd>
                {formatBytes(report.kept)} / {formatBytes(report.budget)}
              </dd>
              <dt className="text-text-secondary">{t("memory.slotsTotal")}</dt>
              <dd>{formatBytes(sumKnown(report.slots.map((slot) => slot.bytes)))}</dd>
              <dt className="text-text-secondary">{t("memory.unattributedTotal")}</dt>
              <dd>{formatBytes(sumKnown(report.unattributed.map((process) => process.bytes)))}</dd>
            </dl>
          </section>

          <section className="card stack">
            <h2 className="text-title-sm">{t("memory.slots")}</h2>
            <table className="w-full table-fixed text-left">
              <thead className="text-text-secondary">
                <tr>
                  <th className="w-16 font-normal">{t("memory.slot")}</th>
                  <th className="font-normal">{t("memory.tab")}</th>
                  <th className="w-24 font-normal">{t("memory.state")}</th>
                  <th className="w-24 font-normal">{t("memory.position")}</th>
                  <th className="w-48 font-normal">{t("memory.loss")}</th>
                  <th className="w-24 text-right font-normal">{t("memory.memory")}</th>
                </tr>
              </thead>
              <tbody>
                {report.slots.map((slot) => {
                  const tab = tabs.find((candidate) => candidate.id === slot.tab);
                  return (
                    <tr key={slot.slot}>
                      <td>{slot.slot}</td>
                      <td className="truncate">{tab ? tabLabel(tab) : "—"}</td>
                      <td>{t(stateLabel(tab))}</td>
                      <td>{slot.position ? t(POSITION_LABELS[slot.position]) : "—"}</td>
                      <td className="truncate" title={lossLabel(slot)}>
                        {lossLabel(slot)}
                      </td>
                      <td className="text-right tabular-nums">{formatBytes(slot.bytes)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </section>

          <section className="card stack">
            <h2 className="text-title-sm">{t("memory.unattributed")}</h2>
            <table className="w-full table-fixed text-left">
              <thead className="text-text-secondary">
                <tr>
                  <th className="font-normal">{t("memory.process")}</th>
                  <th className="w-24 font-normal">{t("memory.pid")}</th>
                  <th className="w-24 text-right font-normal">{t("memory.memory")}</th>
                </tr>
              </thead>
              <tbody>
                {report.unattributed.map((process) => (
                  <tr key={process.pid}>
                    <td>{t(PROCESS_LABELS[process.kind])}</td>
                    <td className="tabular-nums">{process.pid}</td>
                    <td className="text-right tabular-nums">{formatBytes(process.bytes)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        </>
      )}
    </InternalPage>
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
      const first = await unwrap(commands.memoryReport());
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
function lossLabel(slot: SlotMemory): string {
  if (slot.loss === null) {
    return "—";
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
