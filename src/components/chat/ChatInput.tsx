import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowUp, FileStack } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { agentApi } from "@/lib/tauri";
import type { Agent, AttachmentKind } from "@/types/agent";

type Props = {
  disabled?: boolean;
  disabledReason?: string;
  onSend: (text: string) => void;
  /** Bump `token` to set the input value externally (e.g. starter chips).
   *  We watch the token rather than the text so the same prompt can be
   *  applied twice in a row. */
  prefill?: { text: string; token: number };
  /** The currently active agent. Used to read attachment state and bind
   *  attachment writes. When null, the attachment row is hidden. */
  agent?: Agent | null;
  /** Attachment kinds the agent's active skills accept. The button only
   *  appears for kinds in this list. Empty / undefined → no buttons. */
  acceptsAttachments?: string[];
  /** Called after a successful attachment write so the parent can refresh
   *  its agent state. */
  onAgentUpdated?: (agent: Agent) => void;
  /** Quiet status line shown below the input (template + model). Rendered
   *  inside the same surface band so it looks like part of the input area,
   *  not a separate footer panel. */
  footer?: { templateName: string | null; model: string | null };
};

const ATTACHMENT_CONFIG: Record<
  AttachmentKind,
  { label: string; dialogTitle: string; extensions: string[] }
> = {
  template: {
    label: "Vorlage",
    dialogTitle: "Vorlage wählen",
    extensions: ["docx"],
  },
};

export function ChatInput({
  disabled,
  disabledReason,
  onSend,
  prefill,
  agent,
  acceptsAttachments,
  onAgentUpdated,
  footer,
}: Props) {
  const [value, setValue] = useState("");
  const ref = useRef<HTMLTextAreaElement>(null);
  const lastTokenRef = useRef<number | null>(null);

  useEffect(() => {
    if (!prefill) return;
    if (lastTokenRef.current === prefill.token) return;
    lastTokenRef.current = prefill.token;
    setValue(prefill.text);
    ref.current?.focus();
  }, [prefill]);

  function handleSend() {
    const trimmed = value.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    setValue("");
  }

  function handleKey(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSend();
    }
  }

  const visibleSlots = (acceptsAttachments ?? []).filter(
    (k): k is AttachmentKind => k in ATTACHMENT_CONFIG,
  );

  return (
    <div className="border-t border-border bg-surface p-3">
      <div className="relative rounded-md border border-border bg-background focus-within:border-ring focus-within:ring-1 focus-within:ring-ring">
        <textarea
          ref={ref}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKey}
          disabled={disabled}
          placeholder={
            disabled
              ? (disabledReason ?? "Chat ist deaktiviert.")
              : "Schreib eine Nachricht …  (⌘/Ctrl + Enter zum Senden)"
          }
          rows={3}
          className={`block w-full resize-none rounded-md bg-transparent px-3 py-2 pr-10 text-sm placeholder:text-muted-foreground focus:outline-none disabled:cursor-not-allowed disabled:opacity-60 ${
            visibleSlots.length > 0 ? "pb-9" : ""
          }`}
        />
        {visibleSlots.length > 0 && agent && (
          <div className="absolute bottom-1.5 left-1.5 flex items-center gap-1">
            {visibleSlots.map((kind) => (
              <AttachmentButton
                key={kind}
                kind={kind}
                agent={agent}
                onAgentUpdated={onAgentUpdated}
              />
            ))}
          </div>
        )}
        <Button
          size="icon"
          className="absolute bottom-1.5 right-1.5 h-7 w-7"
          onClick={handleSend}
          disabled={disabled || value.trim().length === 0}
          title="Senden (⌘/Ctrl + Enter)"
        >
          <ArrowUp className="h-3.5 w-3.5" />
        </Button>
      </div>
      {footer && (footer.templateName || footer.model) && (
        <div className="mt-1 flex items-center justify-end gap-2 text-[11px] text-muted-foreground">
          {footer.templateName && (
            <>
              <span
                className="min-w-0 truncate"
                title={`Vorlage: ${footer.templateName}`}
              >
                Vorlage: {footer.templateName}
              </span>
              {footer.model && <span className="opacity-40">·</span>}
            </>
          )}
          {footer.model && (
            <span className="shrink-0" title={`Modell: ${footer.model}`}>
              {footer.model}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function attachmentPath(agent: Agent, kind: AttachmentKind): string | null {
  switch (kind) {
    case "template":
      return agent.attachments.templatePath ?? null;
  }
}

function AttachmentButton({
  kind,
  agent,
  onAgentUpdated,
}: {
  kind: AttachmentKind;
  agent: Agent;
  onAgentUpdated?: (agent: Agent) => void;
}) {
  const config = ATTACHMENT_CONFIG[kind];
  const current = attachmentPath(agent, kind);
  const hasAttachment = current !== null;
  const fileName = current ? current.split(/[/\\]/).pop() : null;

  async function handlePick() {
    try {
      const picked = await open({
        directory: false,
        multiple: false,
        title: config.dialogTitle,
        defaultPath: agent.folder ?? undefined,
        filters: [{ name: config.label, extensions: config.extensions }],
      });
      if (typeof picked !== "string") return;
      const updated = await agentApi.setAttachment(agent.id, kind, picked);
      onAgentUpdated?.(updated);
    } catch (e) {
      // Backend rejects paths outside the agent folder; show inline log only —
      // a toast would feel heavy for a single misclick.
      console.warn("attachment pick failed", e);
    }
  }

  // Ghost variant on the left, distinct from the filled primary Send button
  // on the right. Color: warning (amber, with a soft tint) when no path is set
  // or after auto-clear, muted-foreground when valid.
  const tone = hasAttachment
    ? "text-muted-foreground bg-muted hover:bg-accent/60"
    : "text-warning bg-warning/10 hover:bg-warning/20";
  const tooltipText = hasAttachment
    ? `${config.label}: ${fileName}`
    : `${config.label} wählen`;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={handlePick}
          aria-label={tooltipText}
          className={`flex h-7 w-7 items-center justify-center rounded-md transition-colors ${tone}`}
        >
          <FileStack className="h-3.5 w-3.5" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="top">{tooltipText}</TooltipContent>
    </Tooltip>
  );
}
