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
  /** Most recently completed item — used to render "Zeile X von N: <label>". */
  lastItem?: { index: number; label: string };
  lastError?: string;
  /** True after the terminal `delegationFinished` event has arrived. */
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

type StreamState = {
  runId: string;
  assistantMessageId: string;
  buffer: string;
  reasoning: string;
};

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

  const streamRef = useRef<StreamState | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);

  const resetStream = useCallback(() => {
    if (unlistenRef.current) {
      unlistenRef.current();
      unlistenRef.current = null;
    }
    streamRef.current = null;
    setStreamingText(null);
    setStreamingReasoning(null);
    setPendingTools([]);
    setPendingHitl(null);
    setPendingQuestion(null);
    setSending(false);
  }, []);

  // Load history whenever the active agent changes.
  useEffect(() => {
    resetStream();
    setError(null);
    if (!agent) {
      setMessages([]);
      return;
    }
    let cancelled = false;
    chatApi
      .listMessages(agent.id)
      .then((msgs) => {
        if (!cancelled) setMessages(msgs);
      })
      .catch((e) => {
        if (!cancelled)
          setError(String((e as { message?: string })?.message ?? e));
      });
    return () => {
      cancelled = true;
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

      let started;
      try {
        started = await chatApi.sendMessage({
          agentId: agent.id,
          provider: effectiveModel.provider,
          modelId: effectiveModel.modelId,
          text,
        });
      } catch (e) {
        setMessages((prev) => prev.filter((m) => m.id !== tempUserId));
        setSending(false);
        setStreamingText(null);
        setError(String((e as { message?: string })?.message ?? e));
        return;
      }

      streamRef.current = {
        runId: started.runId,
        assistantMessageId: started.assistantMessageId,
        buffer: "",
        reasoning: "",
      };

      try {
        const unlisten = await chatApi.subscribeRun(started.runId, (event) => {
          const state = streamRef.current;
          if (!state || state.runId !== started.runId) return;

          switch (event.type) {
            case "delta":
              state.buffer += event.text;
              setStreamingText(state.buffer);
              break;
            case "reasoningDelta":
              state.reasoning += event.text;
              setStreamingReasoning(state.reasoning);
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
              // A new tool call starts the next LLM iteration from a clean
              // text slate; drop the accumulated streaming text so chips
              // group visually with their request.
              state.buffer = "";
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
                      lastItem: {
                        index: event.index,
                        label: event.itemLabel,
                      },
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
                      lastItem: {
                        index: event.index,
                        label: event.itemLabel,
                      },
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
              chatApi
                .listMessages(agent.id)
                .then(setMessages)
                .catch(() => {});
              resetStream();
              break;
            case "error":
              setError(`${event.code}: ${event.message}`);
              chatApi
                .listMessages(agent.id)
                .then(setMessages)
                .catch(() => {});
              resetStream();
              break;
          }
        });
        unlistenRef.current = unlisten;
      } catch (e) {
        resetStream();
        setError(String((e as { message?: string })?.message ?? e));
      }
    },
    [agent, effectiveModel, sending, resetStream],
  );

  const cancel = useCallback(async () => {
    const state = streamRef.current;
    if (!state) return;
    try {
      await chatApi.cancelRun(state.runId);
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

  useEffect(() => {
    return () => {
      if (unlistenRef.current) unlistenRef.current();
    };
  }, []);

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
