import { fileApi } from "@/lib/tauri";

type Props = {
  fileId: string;
  fileName: string;
};

/** Fallback for file kinds we can't preview yet. Offers a download via a
 *  pre-signed URL (CLAUDE.md §5 — no direct S3 exposure). */
export function UnsupportedViewer({ fileId, fileName }: Props) {
  const download = async () => {
    try {
      const { url } = await fileApi.downloadUrl(fileId);
      window.open(url, "_blank", "noopener,noreferrer");
    } catch (e) {
      console.warn("download url failed", e);
    }
  };
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
      <div className="text-sm font-medium">{fileName}</div>
      <div className="max-w-md text-xs text-muted-foreground">
        Vorschau für diesen Dateityp ist noch nicht verfügbar.
      </div>
      <button
        onClick={download}
        className="rounded-md border border-border bg-surface px-3 py-1 text-xs shadow-subtle hover:bg-accent"
      >
        Herunterladen
      </button>
    </div>
  );
}
