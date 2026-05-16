import { useEffect, useState } from "react";

import { previewApi, type XlsxPreview } from "@/lib/tauri";
import { cn } from "@/lib/utils";

type Props = {
  fileId: string;
};

type State =
  | { kind: "loading" }
  | { kind: "ready"; data: XlsxPreview }
  | { kind: "error"; message: string };

/** XLSX preview — fetches one sheet at a time as a 2D string grid and
 *  renders it as a plain HTML table with sticky headers (column letters
 *  on top, row numbers on the left). Sheets are switched via tabs which
 *  refetch the new sheet. The backend already trims to 1000×50 cells, so
 *  the table stays snappy without virtualization. */
export function XlsxViewer({ fileId }: Props) {
  const [state, setState] = useState<State>({ kind: "loading" });
  // Active sheet name. `null` means "use the workbook's first sheet" — we
  // don't know the names until the first request returns.
  const [activeSheet, setActiveSheet] = useState<string | null>(null);

  // Reset sheet selection on file change so a new file lands on its own
  // first sheet rather than trying to reuse a sheet name from the previous.
  useEffect(() => {
    setActiveSheet(null);
  }, [fileId]);

  useEffect(() => {
    let cancelled = false;
    setState({ kind: "loading" });
    previewApi
      .xlsx(fileId, activeSheet ?? undefined)
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
  }, [fileId, activeSheet]);

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

  const { data } = state;
  const colCount = Math.max(0, ...data.rows.map((r) => r.length));

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {data.sheets.length > 1 && (
        <SheetTabs
          sheets={data.sheets}
          active={data.activeSheet}
          onChange={setActiveSheet}
        />
      )}
      {data.truncated && (
        <div className="shrink-0 border-b border-border bg-amber-50 px-3 py-1.5 text-xs text-amber-900 dark:bg-amber-950 dark:text-amber-100">
          Vorschau gekürzt — zeigt erste {Math.min(data.rows.length, 1000)} ×{" "}
          {Math.min(colCount, 50)} Zellen von {data.totalRows} ×{" "}
          {data.totalCols}.
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="border-collapse text-xs">
          <thead>
            <tr>
              <th className="sticky left-0 top-0 z-20 min-w-[3rem] border-b border-r border-border bg-surface px-2 py-1 font-normal text-muted-foreground" />
              {Array.from({ length: colCount }, (_, c) => (
                <th
                  key={c}
                  className="sticky top-0 z-10 border-b border-r border-border bg-surface px-2 py-1 text-left font-normal text-muted-foreground"
                >
                  {colLetter(c)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.rows.map((row, r) => (
              <tr key={r}>
                <th className="sticky left-0 z-10 min-w-[3rem] border-b border-r border-border bg-surface px-2 py-1 text-right font-normal text-muted-foreground">
                  {r + 1}
                </th>
                {Array.from({ length: colCount }, (_, c) => (
                  <td
                    key={c}
                    className="border-b border-r border-border px-2 py-1 align-top"
                  >
                    <span className="block max-w-xs truncate" title={row[c]}>
                      {row[c] ?? ""}
                    </span>
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function SheetTabs({
  sheets,
  active,
  onChange,
}: {
  sheets: string[];
  active: string;
  onChange: (sheet: string) => void;
}) {
  return (
    <div className="flex shrink-0 gap-1 overflow-x-auto border-b border-border bg-surface px-2 py-1">
      {sheets.map((s) => (
        <button
          key={s}
          onClick={() => onChange(s)}
          className={cn(
            "shrink-0 rounded-sm px-2 py-0.5 text-xs",
            s === active
              ? "bg-accent text-foreground"
              : "text-muted-foreground hover:bg-accent/60",
          )}
          title={s}
        >
          {s}
        </button>
      ))}
    </div>
  );
}

/** 0-based column index → spreadsheet letter (A, B, …, Z, AA, AB, …). */
function colLetter(c: number): string {
  let n = c + 1;
  let s = "";
  while (n > 0) {
    const rem = (n - 1) % 26;
    s = String.fromCharCode(65 + rem) + s;
    n = Math.floor((n - 1) / 26);
  }
  return s;
}
