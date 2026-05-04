import { useState } from "react";
import { AlertTriangle, Check, ChevronRight, Loader2 } from "lucide-react";

import type { DelegationProgress } from "@/hooks/useAgentChat";
import { iconForTool } from "@/lib/toolIcons";
import { cn } from "@/lib/utils";

export type ToolChipStatus = "running" | "done" | "error";

type Props = {
  name: string;
  status: ToolChipStatus;
  arguments?: unknown;
  result?: string;
  delegation?: DelegationProgress;
};

export function ToolCallChip({
  name,
  status,
  arguments: args,
  result,
  delegation,
}: Props) {
  const [expanded, setExpanded] = useState(false);

  const argsText = (() => {
    if (args === undefined || args === null) return null;
    if (typeof args === "string") return args;
    try {
      return JSON.stringify(args, null, 2);
    } catch {
      return String(args);
    }
  })();

  const canExpand = Boolean(argsText) || Boolean(result);
  const ToolIcon = iconForTool(name);

  return (
    <div
      className={cn(
        "flex flex-col gap-1 rounded-md border px-2.5 py-1.5 text-xs",
        status === "running" &&
          "border-amber-500/40 bg-amber-500/15 text-amber-800 dark:text-amber-200",
        status === "done" &&
          "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
        status === "error" &&
          "border-destructive/40 bg-destructive/15 text-destructive",
      )}
    >
      <button
        type="button"
        onClick={() => canExpand && setExpanded((v) => !v)}
        disabled={!canExpand}
        className="flex w-full items-center gap-1.5 text-left"
      >
        {status === "running" ? (
          <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
        ) : status === "done" ? (
          <Check className="h-3 w-3 shrink-0" />
        ) : (
          <AlertTriangle className="h-3 w-3 shrink-0" />
        )}
        <ToolIcon className="h-3 w-3 shrink-0 opacity-60" />
        <span className="font-mono">{name}</span>
        {canExpand && (
          <ChevronRight
            className={cn(
              "ml-auto h-3 w-3 shrink-0 opacity-60 transition-transform",
              expanded && "rotate-90",
            )}
          />
        )}
      </button>

      {delegation && <DelegationProgressStrip progress={delegation} />}

      {expanded && (
        <div className="mt-1 flex flex-col gap-1.5 text-xs">
          {argsText && (
            <div>
              <div className="opacity-60">Arguments</div>
              <pre className="mt-0.5 max-h-32 overflow-auto rounded-sm bg-background/60 p-1.5 font-mono whitespace-pre-wrap">
                {argsText}
              </pre>
            </div>
          )}
          {result && (
            <div>
              <div className="opacity-60">
                {status === "error" ? "Error" : "Result"}
              </div>
              <pre className="mt-0.5 max-h-40 overflow-auto rounded-sm bg-background/60 p-1.5 font-mono whitespace-pre-wrap">
                {result.length > 2000 ? `${result.slice(0, 2000)}\n…` : result}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function DelegationProgressStrip({
  progress,
}: {
  progress: DelegationProgress;
}) {
  const done = progress.succeeded + progress.failed;
  const pct = progress.total > 0 ? Math.min(100, (done / progress.total) * 100) : 0;
  const currentLine = progress.lastItem
    ? progress.finished
      ? `Fertig: ${progress.succeeded} von ${progress.total} geschrieben${
          progress.failed > 0 ? ` · ${progress.failed} Fehler` : ""
        }`
      : `${progress.lastItem.label} · ${done} von ${progress.total}`
    : `Starte … ${progress.total} ${
        progress.total === 1 ? "Eintrag" : "Einträge"
      }`;

  return (
    <div className="mt-0.5 flex flex-col gap-1">
      <div className="flex items-baseline justify-between gap-2 text-[11px] opacity-90">
        <span className="truncate">{currentLine}</span>
        <span className="shrink-0 font-mono opacity-70">
          {done}/{progress.total}
        </span>
      </div>
      <div className="h-1 w-full overflow-hidden rounded-full bg-background/40">
        <div
          className={cn(
            "h-full transition-[width] duration-200",
            progress.failed > 0 ? "bg-amber-500" : "bg-emerald-500",
          )}
          style={{ width: `${pct}%` }}
        />
      </div>
      {progress.lastError && (
        <div className="truncate text-[11px] opacity-75" title={progress.lastError}>
          Letzter Fehler: {progress.lastError}
        </div>
      )}
    </div>
  );
}
