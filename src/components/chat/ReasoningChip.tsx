import { Brain, ChevronRight, Loader2 } from "lucide-react";
import { useState } from "react";

import { MessageMarkdown } from "@/components/chat/MessageMarkdown";
import { cn } from "@/lib/utils";

type Props = {
  text: string;
  /** When true, render with a "still thinking" pulse and no expand toggle. */
  streaming?: boolean;
};

/** Collapsible chain-of-thought / reasoning chip. */
export function ReasoningChip({ text, streaming }: Props) {
  const [expanded, setExpanded] = useState(false);
  const canExpand = text.trim().length > 0;

  return (
    <div className="flex flex-col gap-1 rounded-md border border-border bg-muted/40 px-2.5 py-1.5 text-xs text-muted-foreground">
      <button
        type="button"
        onClick={() => canExpand && setExpanded((v) => !v)}
        disabled={!canExpand}
        className="flex w-full items-center gap-1.5 text-left"
      >
        {streaming ? (
          <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
        ) : (
          <Brain className="h-3 w-3 shrink-0" />
        )}
        <span>{streaming ? "Denkt …" : "Gedanken"}</span>
        {canExpand && !streaming && (
          <ChevronRight
            className={cn(
              "ml-auto h-3 w-3 shrink-0 opacity-60 transition-transform",
              expanded && "rotate-90",
            )}
          />
        )}
      </button>
      {(expanded || streaming) && canExpand && (
        <div className="mt-1 max-h-64 overflow-auto rounded-sm bg-background/60 p-1.5 text-muted-foreground">
          <MessageMarkdown text={text} className="text-xs leading-[17px]" />
        </div>
      )}
    </div>
  );
}
