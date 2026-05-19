export type ChatMessageRole = "user" | "assistant" | "system" | "tool";

export type HitlPreview =
  | {
      kind: "appendToFile";
      path: string;
      content: string;
      createsFile: boolean;
      /** Last few lines of the existing file, so the reviewer can spot a
       *  format mismatch before approving. Absent for new files. */
      existingTail?: string;
    }
  | {
      kind: "writeDocx";
      path: string;
      blockCount: number;
      previewText: string;
      createsFile: boolean;
    }
  | {
      kind: "appendToDocx";
      path: string;
      blockCount: number;
      previewText: string;
      createsFile: boolean;
      existingTail?: string;
    }
  | {
      kind: "rewriteFile";
      path: string;
      before: string;
      after: string;
      createsFile: boolean;
    }
  | {
      kind: "updateCells";
      path: string;
      sheet: string;
      changes: { cell: string; before: string; after: string }[];
    }
  | {
      kind: "writeXlsx";
      path: string;
      sheet: string;
      rows: string[][];
      createsFile: boolean;
    }
  | {
      kind: "writeDocxFromTemplate";
      templatePath: string;
      outputPath: string;
      replacements: { key: string; value: string }[];
      templatePlaceholders: string[];
      createsFile: boolean;
    }
  | {
      kind: "delegateIntoXlsxColumn";
      path: string;
      sheet: string;
      targetColumn: string;
      targetCreatesColumn: boolean;
      rowCount: number;
      workerLabel: string;
      samplePrompts: { rowLabel: string; renderedPrompt: string }[];
    };

export type HitlDecision =
  | { kind: "approve" }
  | { kind: "reject"; reason?: string };

export interface PendingHitl {
  hitlId: string;
  toolCallId: string;
  toolName: string;
  preview: HitlPreview;
}

export interface PendingQuestion {
  questionId: string;
  toolCallId: string;
  question: string;
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: unknown;
}

export interface ToolResult {
  toolUseId: string;
  content: string;
  isError: boolean;
}

export interface ChatMessage {
  id: string;
  role: ChatMessageRole;
  content: string;
  createdAt: string;
  toolCalls?: ToolCall[];
  toolResults?: ToolResult[];
  /** Chain-of-thought / reasoning extracted from the model's output. */
  reasoning?: string;
}

export interface RunStarted {
  runId: string;
  assistantMessageId: string;
}

export type RunEvent =
  /** Run-Start: Clients sollen den Verlauf neu laden (zeigt den
   *  User-Prompt sofort bei allen Mitgliedern). */
  | { type: "userMessage" }
  | { type: "delta"; text: string }
  | { type: "reasoningDelta"; text: string }
  | {
      type: "toolCallStarted";
      id: string;
      name: string;
      arguments: unknown;
    }
  | {
      type: "toolCallCompleted";
      id: string;
      content: string;
      isError: boolean;
    }
  | {
      type: "hitlRequest";
      hitlId: string;
      toolCallId: string;
      toolName: string;
      preview: HitlPreview;
    }
  | {
      type: "hitlResolved";
      hitlId: string;
      decision: HitlDecision;
    }
  | {
      type: "askUserRequest";
      questionId: string;
      toolCallId: string;
      question: string;
    }
  | {
      type: "askUserResolved";
      questionId: string;
      answer: string;
    }
  | {
      type: "delegationStarted";
      toolCallId: string;
      total: number;
    }
  | {
      type: "delegationItemDone";
      toolCallId: string;
      index: number;
      total: number;
      itemLabel: string;
    }
  | {
      type: "delegationItemFailed";
      toolCallId: string;
      index: number;
      total: number;
      itemLabel: string;
      error: string;
    }
  | {
      type: "delegationFinished";
      toolCallId: string;
      succeeded: number;
      failed: number;
    }
  | {
      type: "finish";
      reason: "stop" | "max_tokens" | "cancelled" | "error" | "tool_use";
      message: ChatMessage;
    }
  | { type: "error"; code: string; message: string };
