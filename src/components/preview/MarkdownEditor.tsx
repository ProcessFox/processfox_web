import { useCallback, useEffect, useRef, useState } from "react";
import { Crepe } from "@milkdown/crepe";

import { fileApi } from "@/lib/tauri";

import type { PreviewStatus } from "./PreviewHeader";
import "./markdown-editor.css";

const SAVE_DEBOUNCE_MS = 800;

type Props = {
  agentId: string;
  filePath: string;
  onStatus: (s: PreviewStatus) => void;
};

type Loaded = {
  kind: "loaded";
  initialContent: string;
  conflict: boolean;
};

type State =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | Loaded;

/** Markdown editor backed by Milkdown Crepe — Obsidian-style live preview
 *  where the rendered view IS the editor. The persistence model mirrors
 *  TextEditor: optimistic-concurrency via mtime, debounced auto-save,
 *  reload banner on external modification. */
export function MarkdownEditor({ agentId, filePath, onStatus }: Props) {
  const [state, setState] = useState<State>({ kind: "loading" });
  // Bump on each successful (re)load so CrepeView remounts with the new
  // content. Crepe doesn't expose `setMarkdown`, so destroy+recreate is the
  // simplest correct strategy.
  const [reloadKey, setReloadKey] = useState(0);

  // Latest markdown lives in a ref so the save callback can read it without
  // having to re-create the editor on each change.
  const contentRef = useRef("");
  const mtimeRef = useRef(0);
  const currentPathRef = useRef(filePath);
  const saveTimerRef = useRef<number | null>(null);

  const load = useCallback(async () => {
    try {
      const res = await fileApi.readTextFile(agentId, filePath);
      if (currentPathRef.current !== filePath) return;
      contentRef.current = res.content;
      mtimeRef.current = res.mtime;
      setState({
        kind: "loaded",
        initialContent: res.content,
        conflict: false,
      });
      setReloadKey((k) => k + 1);
      onStatus({ kind: "idle" });
    } catch (e: unknown) {
      if (currentPathRef.current !== filePath) return;
      const message = errorMessage(e);
      setState({ kind: "error", message });
      onStatus({ kind: "error", message });
    }
  }, [agentId, filePath, onStatus]);

  useEffect(() => {
    currentPathRef.current = filePath;
    setState({ kind: "loading" });
    onStatus({ kind: "idle" });
    void load();
    return () => {
      if (saveTimerRef.current !== null) {
        window.clearTimeout(saveTimerRef.current);
        saveTimerRef.current = null;
      }
    };
  }, [filePath, load, onStatus]);

  const save = useCallback(async () => {
    onStatus({ kind: "saving" });
    try {
      const res = await fileApi.writeTextFile(
        agentId,
        filePath,
        contentRef.current,
        mtimeRef.current,
      );
      if (currentPathRef.current !== filePath) return;
      mtimeRef.current = res.mtime;
      setState((cur) =>
        cur.kind === "loaded" ? { ...cur, conflict: false } : cur,
      );
      onStatus({ kind: "saved" });
    } catch (e: unknown) {
      if (currentPathRef.current !== filePath) return;
      const code = (e as { code?: string })?.code;
      if (code === "mtime_conflict") {
        setState((cur) =>
          cur.kind === "loaded" ? { ...cur, conflict: true } : cur,
        );
        onStatus({ kind: "conflict" });
      } else {
        const message = errorMessage(e);
        onStatus({ kind: "error", message });
      }
    }
  }, [agentId, filePath, onStatus]);

  const onChange = useCallback(
    (markdown: string) => {
      contentRef.current = markdown;
      if (saveTimerRef.current !== null) {
        window.clearTimeout(saveTimerRef.current);
      }
      saveTimerRef.current = window.setTimeout(() => {
        saveTimerRef.current = null;
        void save();
      }, SAVE_DEBOUNCE_MS);
    },
    [save],
  );

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
      <CrepeView
        key={reloadKey}
        initialMarkdown={state.initialContent}
        onChange={onChange}
      />
    </div>
  );
}

function CrepeView({
  initialMarkdown,
  onChange,
}: {
  initialMarkdown: string;
  onChange: (md: string) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  // Latest onChange — kept in a ref so the editor effect doesn't re-create
  // Crepe on every parent re-render.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    const root = containerRef.current;
    if (!root) return;

    const crepe = new Crepe({ root, defaultValue: initialMarkdown });
    crepe.on((api) => {
      api.markdownUpdated((_ctx, md) => {
        onChangeRef.current(md);
      });
    });

    let destroyed = false;
    crepe.create().catch((e) => {
      if (!destroyed) console.error("crepe init failed", e);
    });

    return () => {
      destroyed = true;
      void crepe.destroy();
    };
    // initialMarkdown is captured at mount only; we remount via the parent's
    // `key` whenever a fresh value should take effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div ref={containerRef} className="preview-md-host min-h-0 flex-1" />;
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
