import { useCallback, useEffect, useRef, useState } from "react";

import { chatApi } from "@/lib/tauri";
import type { Agent } from "@/types/agent";
import type { ChatMessage, PendingHitl, PendingQuestion } from "@/types/chat";
import type { Settings } from "@/types/settings";

export type EffectiveModel = {
  provider: string;
  modelId: string;
};

export function resolveAgentModel(
  agent: Agent | null,
  settings: Settings | null,
): EffectiveModel | null {
  if (!agent) return null;
  if (agent.model) {
    if (agent.model.id.trim().length === 0) return null;
    return { provider: agent.model.provider, modelId: agent.model.id };
  }
  const provider = settings?.defaultProvider;
  if (!provider) return null;
  const modelId = settings?.defaultModel;
  if (!modelId) return null;
  return { provider, modelId };
}

export type DelegationProgress = {
  total: number;
  succeeded: number;
  failed: number;
  lastItem?: { index: number; label: string };
  lastError?: string;
  finished: boolean;
};

export type PendingToolCall = {
  id: string;
  name: string;
  arguments: unknown;
  status: "running" | "done" | "error";
  content?: string;
  delegation?: DelegationProgress;
};

/**
 * Shared-Session-Chat: Wir abonnieren **pro aktivem Agenten** den
 * Agenten-Channel — damit sehen alle Workspace-Mitglieder, die den
 * Agenten offen haben, denselben laufenden Run live (Backend erlaubt
 * genau einen aktiven Run je Agent). `send` startet nur den Run; der
 * Stream kommt für alle über den Channel.
 */
export function useAgentChat(
  agent: Agent | null,
  effectiveModel: EffectiveModel | null,
) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [sending, setSending] = useState(false);
  const [streamingText, setStreamingText] = useState<string | null>(null);
  const [streamingReasoning, setStreamingReasoning] = useState<string | null>(
    null,
  );
  const [pendingTools, setPendingTools] = useState<PendingToolCall[]>([]);
  const [pendingHitl, setPendingHitl] = useState<PendingHitl | null>(null);
  const [pendingQuestion, setPendingQuestion] =
    useState<PendingQuestion | null>(null);
  const [error, setError] = useState<string | null>(null);

  const bufferRef = useRef("");
  const reasoningRef = useRef("");
  // Run-ID des zuletzt von DIESEM Client gestarteten Runs (für Cancel).
  const currentRunRef = useRef<string | null>(null);

  const resetStream = useCallback(() => {
    bufferRef.current = "";
    reasoningRef.current = "";
    setStreamingText(null);
    setStreamingReasoning(null);
    setPendingTools([]);
    setPendingHitl(null);
    setPendingQuestion(null);
    setSending(false);
  }, []);

  // Pro Agent: Verlauf laden + Agenten-Channel abonnieren (live für alle).
  useEffect(() => {
    resetStream();
    setError(null);
    currentRunRef.current = null;
    if (!agent) {
      setMessages([]);
      return;
    }
    const agentId = agent.id;
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    chatApi
      .listMessages(agentId)
      .then((msgs) => {
        if (!cancelled) setMessages(msgs);
      })
      .catch((e) => {
        if (!cancelled)
          setError(String((e as { message?: string })?.message ?? e));
      });

    chatApi
      .subscribeAgent(agentId, (event) => {
        switch (event.type) {
          case "userMessage":
            // Run-Start: autoritativen Verlauf laden — Prompt erscheint
            // sofort bei allen; optimistischer Platzhalter wird ersetzt.
            chatApi
              .listMessages(agentId)
              .then(setMessages)
              .catch(() => {});
            break;
          case "delta":
            bufferRef.current += event.text;
            setStreamingText(bufferRef.current);
            break;
          case "reasoningDelta":
            reasoningRef.current += event.text;
            setStreamingReasoning(reasoningRef.current);
            break;
          case "toolCallStarted":
            setPendingTools((prev) => [
              ...prev,
              {
                id: event.id,
                name: event.name,
                arguments: event.arguments,
                status: "running",
              },
            ]);
            bufferRef.current = "";
            setStreamingText("");
            break;
          case "toolCallCompleted":
            setPendingTools((prev) =>
              prev.map((t) =>
                t.id === event.id
                  ? {
                      ...t,
                      status: event.isError ? "error" : "done",
                      content: event.content,
                    }
                  : t,
              ),
            );
            break;
          case "hitlRequest":
            setPendingHitl({
              hitlId: event.hitlId,
              toolCallId: event.toolCallId,
              toolName: event.toolName,
              preview: event.preview,
            });
            break;
          case "hitlResolved":
            setPendingHitl((prev) =>
              prev && prev.hitlId === event.hitlId ? null : prev,
            );
            break;
          case "askUserRequest":
            setPendingQuestion({
              questionId: event.questionId,
              toolCallId: event.toolCallId,
              question: event.question,
            });
            break;
          case "askUserResolved":
            setPendingQuestion((prev) =>
              prev && prev.questionId === event.questionId ? null : prev,
            );
            break;
          case "delegationStarted":
            setPendingTools((prev) =>
              prev.map((t) =>
                t.id === event.toolCallId
                  ? {
                      ...t,
                      delegation: {
                        total: event.total,
                        succeeded: 0,
                        failed: 0,
                        finished: false,
                      },
                    }
                  : t,
              ),
            );
            break;
          case "delegationItemDone":
            setPendingTools((prev) =>
              prev.map((t) => {
                if (t.id !== event.toolCallId || !t.delegation) return t;
                return {
                  ...t,
                  delegation: {
                    ...t.delegation,
                    succeeded: t.delegation.succeeded + 1,
                    lastItem: { index: event.index, label: event.itemLabel },
                  },
                };
              }),
            );
            break;
          case "delegationItemFailed":
            setPendingTools((prev) =>
              prev.map((t) => {
                if (t.id !== event.toolCallId || !t.delegation) return t;
                return {
                  ...t,
                  delegation: {
                    ...t.delegation,
                    failed: t.delegation.failed + 1,
                    lastItem: { index: event.index, label: event.itemLabel },
                    lastError: event.error,
                  },
                };
              }),
            );
            break;
          case "delegationFinished":
            setPendingTools((prev) =>
              prev.map((t) => {
                if (t.id !== event.toolCallId || !t.delegation) return t;
                return {
                  ...t,
                  delegation: {
                    ...t.delegation,
                    succeeded: event.succeeded,
                    failed: event.failed,
                    finished: true,
                  },
                };
              }),
            );
            break;
          case "finish":
            // Phase 6d-1: das Backend persistiert jetzt strukturierte
            // Assistant-/Tool-Rows. Wir warten den Reload ab, **bevor**
            // wir den Live-State (`pendingTools`, `streamingText`,
            // `streamingReasoning`) löschen — sonst flackert die UI
            // zwischen Stream-Ende und persistiertem Verlauf einen
            // Frame lang leer.
            currentRunRef.current = null;
            chatApi
              .listMessages(agentId)
              .then((msgs) => {
                setMessages(msgs);
                resetStream();
              })
              .catch(() => {
                resetStream();
              });
            break;
          case "error":
            setError(`${event.code}: ${event.message}`);
            chatApi.listMessages(agentId).then(setMessages).catch(() => {});
            currentRunRef.current = null;
            resetStream();
            break;
        }
      })
      .then((u) => {
        if (cancelled) u();
        else unlisten = u;
      })
      .catch((e) => console.warn("agent subscribe failed", e));

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [agent, resetStream]);

  const send = useCallback(
    async (text: string) => {
      if (!agent || !effectiveModel || sending) return;
      setError(null);

      const tempUserId = crypto.randomUUID();
      const tempUserMsg: ChatMessage = {
        id: tempUserId,
        role: "user",
        content: text,
        createdAt: new Date().toISOString(),
      };
      setMessages((prev) => [...prev, tempUserMsg]);
      setSending(true);
      setStreamingText("");
      setStreamingReasoning(null);
      setPendingTools([]);

      try {
        const started = await chatApi.sendMessage({
          agentId: agent.id,
          provider: effectiveModel.provider,
          modelId: effectiveModel.modelId,
          text,
        });
        currentRunRef.current = started.runId;
        // Stream kommt über den Agenten-Channel (Effekt oben).
      } catch (e) {
        // Inkl. 409 „Es läuft bereits eine Antwort für diesen Agenten."
        setMessages((prev) => prev.filter((m) => m.id !== tempUserId));
        resetStream();
        setError(String((e as { message?: string })?.message ?? e));
      }
    },
    [agent, effectiveModel, sending, resetStream],
  );

  const cancel = useCallback(async () => {
    const runId = currentRunRef.current;
    if (!runId) return;
    try {
      await chatApi.cancelRun(runId);
    } catch (e) {
      console.warn("cancel failed", e);
    }
  }, []);

  const approveHitl = useCallback(async () => {
    const id = pendingHitl?.hitlId;
    if (!id) return;
    try {
      await chatApi.approveHitl(id);
    } catch (e) {
      console.warn("approve failed", e);
    }
  }, [pendingHitl]);

  const rejectHitl = useCallback(
    async (reason?: string) => {
      const id = pendingHitl?.hitlId;
      if (!id) return;
      try {
        await chatApi.rejectHitl(id, reason);
      } catch (e) {
        console.warn("reject failed", e);
      }
    },
    [pendingHitl],
  );

  const respondToQuestion = useCallback(
    async (answer: string) => {
      const id = pendingQuestion?.questionId;
      if (!id) return;
      try {
        await chatApi.respondToQuestion(id, answer);
      } catch (e) {
        console.warn("respond failed", e);
      }
    },
    [pendingQuestion],
  );

  return {
    messages,
    sending,
    streamingText,
    streamingReasoning,
    pendingTools,
    pendingHitl,
    pendingQuestion,
    error,
    send,
    cancel,
    approveHitl,
    rejectHitl,
    respondToQuestion,
    clearError: () => setError(null),
  } as const;
}
