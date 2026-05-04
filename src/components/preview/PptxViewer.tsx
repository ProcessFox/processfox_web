import { useEffect, useState } from "react";

import { previewApi, type PptxPreview, type SlidePreview } from "@/lib/tauri";

type Props = {
  agentId: string;
  filePath: string;
};

type State =
  | { kind: "loading" }
  | { kind: "ready"; data: PptxPreview }
  | { kind: "error"; message: string };

/** PPTX preview — text-only outline. Each slide is rendered as a card with
 *  its title, body bullets, and (collapsed by default) speaker notes. We
 *  intentionally don't try to recreate the visual layout — see
 *  `core::preview::pptx` for why. */
export function PptxViewer({ agentId, filePath }: Props) {
  const [state, setState] = useState<State>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    setState({ kind: "loading" });
    previewApi
      .pptx(agentId, filePath)
      .then((data) => {
        if (cancelled) return;
        setState({ kind: "ready", data });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        const message =
          (e as { message?: string })?.message ?? String(e ?? "Unbekannter Fehler");
        setState({ kind: "error", message });
      });
    return () => {
      cancelled = true;
    };
  }, [agentId, filePath]);

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

  const { slides } = state.data;
  if (slides.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-center text-xs text-muted-foreground">
        Diese Präsentation enthält keine Folien.
      </div>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-auto bg-muted/30 p-4">
      <div className="mx-auto flex max-w-3xl flex-col gap-3">
        {slides.map((slide) => (
          <SlideCard key={slide.index} slide={slide} />
        ))}
      </div>
    </div>
  );
}

function SlideCard({ slide }: { slide: SlidePreview }) {
  const [notesOpen, setNotesOpen] = useState(false);
  const titleText = slide.title?.trim() || `Folie ${slide.index}`;
  return (
    <div className="rounded-md border border-border bg-background p-4 shadow-subtle">
      <div className="mb-2 flex items-baseline gap-2">
        <span className="text-xs tabular-nums text-muted-foreground">
          {slide.index}
        </span>
        <h2 className="text-sm font-medium">{titleText}</h2>
      </div>
      {slide.body.length > 0 && (
        <ul className="ml-4 list-disc space-y-1 text-sm leading-relaxed">
          {slide.body.map((b, i) => (
            <li key={i} className="whitespace-pre-wrap">
              {b}
            </li>
          ))}
        </ul>
      )}
      {slide.body.length === 0 && !slide.title && (
        <div className="text-xs italic text-muted-foreground">
          Kein Text auf dieser Folie.
        </div>
      )}
      {slide.notes.length > 0 && (
        <div className="mt-3 border-t border-border pt-2">
          <button
            onClick={() => setNotesOpen((v) => !v)}
            className="text-xs text-muted-foreground hover:text-foreground"
          >
            {notesOpen ? "▾" : "▸"} Notizen ({slide.notes.length})
          </button>
          {notesOpen && (
            <div className="mt-2 space-y-1 text-xs text-muted-foreground">
              {slide.notes.map((n, i) => (
                <p key={i} className="whitespace-pre-wrap">
                  {n}
                </p>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
