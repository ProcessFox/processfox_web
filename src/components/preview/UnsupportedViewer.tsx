import { openPath } from "@tauri-apps/plugin-opener";

type Props = {
  filePath: string;
  fileName: string;
};

/** Fallback for file kinds we can't preview yet. Offers to open the file in
 *  the OS-default app via `tauri-plugin-opener`. */
export function UnsupportedViewer({ filePath, fileName }: Props) {
  const openExternally = () => {
    openPath(filePath).catch((e) => console.warn("open failed", e));
  };
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
      <div className="text-sm font-medium">{fileName}</div>
      <div className="max-w-md text-xs text-muted-foreground">
        Vorschau für diesen Dateityp ist noch nicht verfügbar.
      </div>
      <button
        onClick={openExternally}
        className="rounded-md border border-border bg-surface px-3 py-1 text-xs shadow-subtle hover:bg-accent"
      >
        In Standard-App öffnen
      </button>
    </div>
  );
}
