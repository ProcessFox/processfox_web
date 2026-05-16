import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { ArrowUp, FileStack } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { agentApi, fileApi } from "@/lib/tauri";
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
  { label: string; accept: string }
> = {
  template: {
    label: "Vorlage",
    accept: ".docx",
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

function attachmentFileId(agent: Agent, kind: AttachmentKind): string | null {
  switch (kind) {
    case "template":
      return agent.attachments.templateFileId ?? null;
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
  const current = attachmentFileId(agent, kind);
  const hasAttachment = current !== null;
  const inputRef = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);

  async function handleFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    setBusy(true);
    try {
      // Upload into the agent's workspace, then bind the resulting file id
      // as the attachment (CLAUDE.md §5 — upload replaces OS file access).
      const uploaded = await fileApi.upload(agent.workspaceId, file);
      const updated = await agentApi.setAttachment(agent.id, kind, uploaded.id);
      onAgentUpdated?.(updated);
    } catch (err) {
      console.warn("attachment upload failed", err);
    } finally {
      setBusy(false);
    }
  }

  // Ghost variant on the left, distinct from the filled primary Send button
  // on the right. Color: warning (amber, with a soft tint) when nothing is
  // attached yet, muted-foreground when set.
  const tone = hasAttachment
    ? "text-muted-foreground bg-muted hover:bg-accent/60"
    : "text-warning bg-warning/10 hover:bg-warning/20";
  const tooltipText = hasAttachment
    ? `${config.label} gesetzt — zum Ersetzen klicken`
    : `${config.label} wählen`;

  return (
    <>
      <input
        ref={inputRef}
        type="file"
        accept={config.accept}
        className="hidden"
        onChange={handleFile}
      />
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={() => inputRef.current?.click()}
            disabled={busy}
            aria-label={tooltipText}
            className={`flex h-7 w-7 items-center justify-center rounded-md transition-colors disabled:opacity-50 ${tone}`}
          >
            <FileStack className="h-3.5 w-3.5" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="top">{tooltipText}</TooltipContent>
      </Tooltip>
    </>
  );
}
