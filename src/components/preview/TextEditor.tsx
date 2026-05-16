import { useCallback, useEffect, useRef, useState } from "react";

import { fileApi } from "@/lib/tauri";

import type { PreviewStatus } from "./PreviewHeader";

const SAVE_DEBOUNCE_MS = 800;

type Props = {
  fileId: string;
  onStatus: (s: PreviewStatus) => void;
};

type Loaded = {
  kind: "loaded";
  content: string;
  /** Server-known version token (e.g. S3 ETag). Used as the optimistic-
   *  concurrency token on save and updated whenever a save round-trips. */
  version: string;
  conflict: boolean;
};

type State =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | Loaded;

/** Plain-text / source-code editor with debounced auto-save and external-
 *  modification detection. The textarea is the source of truth for content;
 *  the backend's mtime is the source of truth for "did anyone else write
 *  to this file since we read it?" — if so, the next save fails with a
 *  `mtime_conflict` and we offer a Reload button. */
export function TextEditor({ fileId, onStatus }: Props) {
  const [state, setState] = useState<State>({ kind: "loading" });

  // File id the latest async response is allowed to apply to. Guards against
  // a slow read/write resolving after the user has switched files.
  const currentIdRef = useRef(fileId);
  const saveTimerRef = useRef<number | null>(null);

  const load = useCallback(async () => {
    try {
      const res = await fileApi.readTextFile(fileId);
      if (currentIdRef.current !== fileId) return;
      setState({
        kind: "loaded",
        content: res.content,
        version: res.version,
        conflict: false,
      });
      onStatus({ kind: "idle" });
    } catch (e: unknown) {
      if (currentIdRef.current !== fileId) return;
      const message = errorMessage(e);
      setState({ kind: "error", message });
      onStatus({ kind: "error", message });
    }
  }, [fileId, onStatus]);

  // Reset on file change, then load.
  useEffect(() => {
    currentIdRef.current = fileId;
    setState({ kind: "loading" });
    onStatus({ kind: "idle" });
    void load();
    return () => {
      if (saveTimerRef.current !== null) {
        window.clearTimeout(saveTimerRef.current);
        saveTimerRef.current = null;
      }
    };
  }, [fileId, load, onStatus]);

  const save = useCallback(async () => {
    setState((prev) => {
      if (prev.kind !== "loaded") return prev;
      doSave(prev);
      return prev;
    });

    async function doSave(loaded: Loaded) {
      onStatus({ kind: "saving" });
      try {
        const res = await fileApi.writeTextFile(
          fileId,
          loaded.content,
          loaded.version,
        );
        if (currentIdRef.current !== fileId) return;
        setState((cur) =>
          cur.kind === "loaded"
            ? { ...cur, version: res.version, conflict: false }
            : cur,
        );
        onStatus({ kind: "saved" });
      } catch (e: unknown) {
        if (currentIdRef.current !== fileId) return;
        const code = (e as { code?: string })?.code;
        if (code === "version_conflict") {
          setState((cur) =>
            cur.kind === "loaded" ? { ...cur, conflict: true } : cur,
          );
          onStatus({ kind: "conflict" });
        } else {
          const message = errorMessage(e);
          onStatus({ kind: "error", message });
        }
      }
    }
  }, [fileId, onStatus]);

  const onChange = (value: string) => {
    setState((prev) =>
      prev.kind === "loaded" ? { ...prev, content: value } : prev,
    );
    if (saveTimerRef.current !== null) {
      window.clearTimeout(saveTimerRef.current);
    }
    saveTimerRef.current = window.setTimeout(() => {
      saveTimerRef.current = null;
      void save();
    }, SAVE_DEBOUNCE_MS);
  };

  if (state.kind === "loading") {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-xs text-muted-foreground">
        Lädt …
      </div>
    );
  }
  if (state.kind === "error") {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-center text-xs text-destructive">
        {state.message}
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {state.conflict && <ConflictBanner onReload={() => void load()} />}
      <textarea
        value={state.content}
        onChange={(e) => onChange(e.target.value)}
        className="flex-1 resize-none border-0 bg-background p-4 font-mono text-sm leading-relaxed outline-none"
        spellCheck={false}
      />
    </div>
  );
}

function ConflictBanner({ onReload }: { onReload: () => void }) {
  return (
    <div className="flex items-center justify-between gap-3 border-b border-border bg-amber-50 px-3 py-2 text-xs text-amber-900 dark:bg-amber-950 dark:text-amber-100">
      <span>
        Datei wurde extern geändert. Deine Änderungen sind noch nicht
        gespeichert.
      </span>
      <button
        onClick={onReload}
        className="rounded-md border border-amber-300 bg-white px-2 py-0.5 text-amber-900 hover:bg-amber-100 dark:border-amber-800 dark:bg-amber-900 dark:text-amber-100 dark:hover:bg-amber-800"
      >
        Neu laden
      </button>
    </div>
  );
}

function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
