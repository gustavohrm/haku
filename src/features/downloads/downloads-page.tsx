import { InternalPage } from "@app/internal-page";
import { commands } from "@ipc/commands";
import { useDownloads } from "@ipc/hooks";
import { t } from "@shared/i18n";

import { DownloadItem } from "./download-item";

/**
 * Every download this window's profile lists, newest first, with what the
 * toolbar's list offers and a way to clear the ones that ended. Clearing
 * unlists them; their files stay.
 */
export function DownloadsPage() {
  const downloads = useDownloads();
  const ended = downloads.some((download) => download.state !== "inProgress" && download.state !== "paused");

  return (
    <InternalPage
      title={t("downloads.title")}
      actions={
        <button
          type="button"
          className="btn ghost edged"
          disabled={!ended}
          onClick={() => void commands.clearDownloads()}
        >
          {t("downloads.clear")}
        </button>
      }
    >
      {downloads.length === 0 ? (
        <p className="text-text-secondary">{t("downloads.empty")}</p>
      ) : (
        <ul className="flex flex-col">
          {downloads.map((download) => (
            <DownloadItem key={download.id} download={download} />
          ))}
        </ul>
      )}
    </InternalPage>
  );
}
