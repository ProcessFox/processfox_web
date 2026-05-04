import { useEffect, useRef, useState } from "react";
import { Document, Page, pdfjs } from "react-pdf";
import { ChevronLeft, ChevronRight, ZoomIn, ZoomOut } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";

import "react-pdf/dist/Page/TextLayer.css";
import "react-pdf/dist/Page/AnnotationLayer.css";

// Vite resolves this to a final asset URL the PDF.js worker can be loaded
// from. Configured once at module load — `pdfjs` is a singleton.
pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  "pdfjs-dist/build/pdf.worker.min.mjs",
  import.meta.url,
).toString();

const MIN_SCALE = 0.5;
const MAX_SCALE = 3;
const SCALE_STEP = 0.2;

type Props = {
  filePath: string;
};

export function PdfViewer({ filePath }: Props) {
  const [numPages, setNumPages] = useState<number | null>(null);
  const [pageNumber, setPageNumber] = useState(1);
  const [scale, setScale] = useState(1);
  const [error, setError] = useState<string | null>(null);

  const containerRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState<number | null>(null);

  // Track container width so the PDF page renders crisp at the available
  // size (the asset URL is fed straight into PDF.js). Rerun on container
  // resize via ResizeObserver.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const update = () => setContainerWidth(el.clientWidth);
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Reset on path change.
  useEffect(() => {
    setNumPages(null);
    setPageNumber(1);
    setError(null);
  }, [filePath]);

  const src = convertFileSrc(filePath);

  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-center text-xs text-destructive">
        PDF konnte nicht geladen werden: {error}
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Toolbar
        page={pageNumber}
        numPages={numPages}
        scale={scale}
        onPrev={() => setPageNumber((p) => Math.max(1, p - 1))}
        onNext={() =>
          setPageNumber((p) => (numPages ? Math.min(numPages, p + 1) : p))
        }
        onZoomIn={() => setScale((s) => Math.min(MAX_SCALE, s + SCALE_STEP))}
        onZoomOut={() => setScale((s) => Math.max(MIN_SCALE, s - SCALE_STEP))}
      />
      <div
        ref={containerRef}
        className="flex flex-1 items-start justify-center overflow-auto bg-muted/40 p-4"
      >
        <Document
          file={src}
          onLoadSuccess={({ numPages }) => setNumPages(numPages)}
          onLoadError={(err) => setError(err.message)}
          loading={
            <div className="text-xs text-muted-foreground">Lädt …</div>
          }
        >
          {containerWidth !== null && (
            <Page
              pageNumber={pageNumber}
              // Subtract padding so the page fits the visible area without
              // a horizontal scrollbar at default zoom.
              width={Math.max(200, (containerWidth - 32) * scale)}
              renderAnnotationLayer={false}
              renderTextLayer={false}
              className="shadow-md"
            />
          )}
        </Document>
      </div>
    </div>
  );
}

function Toolbar({
  page,
  numPages,
  scale,
  onPrev,
  onNext,
  onZoomIn,
  onZoomOut,
}: {
  page: number;
  numPages: number | null;
  scale: number;
  onPrev: () => void;
  onNext: () => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
}) {
  return (
    <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border bg-surface px-3 py-1.5 text-xs">
      <div className="flex items-center gap-1">
        <ToolbarButton
          onClick={onPrev}
          disabled={page <= 1}
          title="Vorherige Seite"
        >
          <ChevronLeft className="h-3.5 w-3.5" />
        </ToolbarButton>
        <span className="px-1 tabular-nums text-muted-foreground">
          {page} / {numPages ?? "…"}
        </span>
        <ToolbarButton
          onClick={onNext}
          disabled={numPages === null || page >= numPages}
          title="Nächste Seite"
        >
          <ChevronRight className="h-3.5 w-3.5" />
        </ToolbarButton>
      </div>
      <div className="flex items-center gap-1">
        <ToolbarButton
          onClick={onZoomOut}
          disabled={scale <= MIN_SCALE + 1e-3}
          title="Verkleinern"
        >
          <ZoomOut className="h-3.5 w-3.5" />
        </ToolbarButton>
        <span className="w-10 text-center tabular-nums text-muted-foreground">
          {Math.round(scale * 100)}%
        </span>
        <ToolbarButton
          onClick={onZoomIn}
          disabled={scale >= MAX_SCALE - 1e-3}
          title="Vergrößern"
        >
          <ZoomIn className="h-3.5 w-3.5" />
        </ToolbarButton>
      </div>
    </div>
  );
}

function ToolbarButton({
  onClick,
  disabled,
  title,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  title?: string;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      className="flex h-6 w-6 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent disabled:opacity-30 disabled:hover:bg-transparent"
    >
      {children}
    </button>
  );
}
