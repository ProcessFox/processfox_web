import { useCallback, useEffect, useRef, useState } from "react";
import { Trash2, Upload } from "lucide-react";

import { iconForFile } from "@/lib/fileIcons";
import { fileApi } from "@/lib/tauri";
import type { WorkspaceFile } from "@/types/file";
import { cn } from "@/lib/utils";

type Props = {
  /** Active workspace. Files live flat under `workspaces/<id>/`. */
  workspaceId: string | null;
  /** Bump to force a refetch (e.g. after a chat message is sent). */
  refreshSignal?: number;
  onSelectFile: (fileId: string, name: string) => void;
};

export function FileTree({
  workspaceId,
  refreshSignal,
  onSelectFile,
}: Props) {
  const [files, setFiles] = useState<WorkspaceFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const refresh = useCallback(() => {
    if (!workspaceId) {
      setFiles([]);
      return;
    }
    setLoading(true);
    setError(null);
    fileApi
      .list(workspaceId)
      .then(setFiles)
      .catch((err) =>
        setError(typeof err === "string" ? err : (err?.message ?? String(err))),
      )
      .finally(() => setLoading(false));
  }, [workspaceId]);

  useEffect(() => {
    refresh();
  }, [refresh, refreshSignal]);

  // Live update: the backend broadcasts `fs-changed` to every workspace
  // member whenever a file is added/removed (CLAUDE.md §5).
  useEffect(() => {
    if (!workspaceId) return;
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    fileApi
      .subscribeFsChanged(() => refresh())
      .then((u) => {
        if (cancelled) u();
        else unlisten = u;
      })
      .catch((e) => console.warn("fs-changed subscribe failed", e));
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [workspaceId, refresh]);

  const uploadFiles = useCallback(
    async (list: FileList | File[]) => {
      if (!workspaceId) return;
      setUploading(true);
      try {
        for (const file of Array.from(list)) {
          await fileApi.upload(workspaceId, file);
        }
        refresh();
      } catch (e) {
        setError(typeof e === "string" ? e : ((e as Error)?.message ?? String(e)));
      } finally {
        setUploading(false);
      }
    },
    [workspaceId, refresh],
  );

  async function handleDelete(file: WorkspaceFile) {
    try {
      await fileApi.delete(file.id);
      refresh();
    } catch (e) {
      console.warn("delete failed", e);
    }
  }

  if (!workspaceId) {
    return (
      <EmptyState
        title="Kein Workspace ausgewählt"
        description="Wähle oben einen Workspace."
      />
    );
  }

  return (
    <div
      className="flex h-full w-full flex-col overflow-hidden p-2"
      onDragOver={(e) => {
        e.preventDefault();
        setDragOver(true);
      }}
      onDragLeave={() => setDragOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setDragOver(false);
        if (e.dataTransfer.files.length > 0) {
          void uploadFiles(e.dataTransfer.files);
        }
      }}
    >
      <div className="flex items-center justify-between gap-2 px-2 pb-2">
        <span className="text-xs font-medium text-muted-foreground">
          Dateien
        </span>
        <button
          onClick={() => inputRef.current?.click()}
          disabled={uploading}
          className="flex items-center gap-1 rounded-sm border border-border bg-surface px-2 py-0.5 text-xs hover:bg-accent disabled:opacity-50"
          title="Datei hochladen"
        >
          <Upload className="h-3 w-3" />
          {uploading ? "Lädt hoch …" : "Upload"}
        </button>
        <input
          ref={inputRef}
          type="file"
          multiple
          className="hidden"
          onChange={(e) => {
            if (e.target.files) void uploadFiles(e.target.files);
            e.target.value = "";
          }}
        />
      </div>

      <div
        className={cn(
          "flex-1 overflow-auto rounded-sm",
          dragOver && "ring-2 ring-primary ring-inset",
        )}
      >
        {loading ? (
          <div className="px-3 py-2 text-xs text-muted-foreground">Lädt …</div>
        ) : error ? (
          <div className="px-3 py-2 text-xs text-destructive">
            Fehler: {error}
          </div>
        ) : files.length === 0 ? (
          <div className="px-3 py-6 text-center text-xs text-muted-foreground">
            Keine Dateien. Zum Hochladen hierher ziehen.
          </div>
        ) : (
          files.map((f) => {
            const Icon = iconForFile(f.filename);
            return (
              <div
                key={f.id}
                className="group flex h-[26px] cursor-pointer select-none items-center gap-1.5 rounded-sm px-2 text-sm hover:bg-accent/60"
                onClick={() => onSelectFile(f.id, f.filename)}
              >
                <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                <span
                  className="min-w-0 flex-1 truncate"
                  title={f.filename}
                >
                  {f.filename}
                </span>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    void handleDelete(f);
                  }}
                  className="hidden h-5 w-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-destructive/15 hover:text-destructive group-hover:flex"
                  title="Löschen"
                >
                  <Trash2 className="h-3 w-3" />
                </button>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

function EmptyState({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 px-4 text-center">
      <div className="text-sm font-medium">{title}</div>
      <div className="text-xs text-muted-foreground">{description}</div>
    </div>
  );
}
