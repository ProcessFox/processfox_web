/**
 * Web API bridge — drop-in replacement for the Tauri invoke/listen bridge.
 *
 * All functions keep the same signatures as the original tauri.ts so that
 * the rest of the frontend needs no changes. Implementations are wired to
 * HTTP POST /api/<command> and WebSocket /ws/<channel>.
 */

import type {
  Agent,
  AgentDraft,
  AgentUpdate,
  AttachmentKind,
} from "@/types/agent";
import type { ChatMessage, RunEvent, RunStarted } from "@/types/chat";
import type { FileEntry } from "@/types/file";
import type {
  CatalogEntry,
  DownloadEvent,
  HardwareInfo,
  InstalledModel,
} from "@/types/models";
import type { Settings } from "@/types/settings";
import type { Skill } from "@/types/skill";

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

const BASE = "/api";

async function post<T>(command: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}/${command}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ message: res.statusText }));
    throw new Error((err as { message?: string }).message ?? res.statusText);
  }
  return res.json() as Promise<T>;
}

export type UnlistenFn = () => void;

function subscribeWs<T>(
  channel: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  const protocol = location.protocol === "https:" ? "wss:" : "ws:";
  const ws = new WebSocket(`${protocol}//${location.host}/ws/${channel}`);
  ws.onmessage = (evt) => handler(JSON.parse(evt.data) as T);
  return Promise.resolve(() => ws.close());
}

// ---------------------------------------------------------------------------
// Public APIs — same surface as the original Tauri bridge
// ---------------------------------------------------------------------------

export const agentApi = {
  list: () => post<Agent[]>("list_agents"),
  get: (id: string) => post<Agent>("get_agent", { id }),
  create: (draft: AgentDraft) => post<Agent>("create_agent", { draft }),
  update: (id: string, update: AgentUpdate) =>
    post<Agent>("update_agent", { id, update }),
  delete: (id: string) => post<void>("delete_agent", { id }),
  setAttachment: (agentId: string, kind: AttachmentKind, path: string | null) =>
    post<Agent>("set_agent_attachment", { agentId, kind, path }),
  subscribeAttachmentsChanged: (
    handler: (agentId: string) => void,
  ): Promise<UnlistenFn> =>
    subscribeWs<string>("agent-attachments-changed", handler),
};

export interface TextFileContent {
  content: string;
  /** Modification time in ms since UNIX epoch — pass back unchanged on save
   *  so the backend can detect external modifications. */
  mtime: number;
}

export interface TextWriteResult {
  mtime: number;
}

export interface DocxPreview {
  /** Sanitised body-only HTML — render in an iframe with `sandbox=""`. */
  html: string;
}

export interface XlsxPreview {
  sheets: string[];
  activeSheet: string;
  rows: string[][];
  totalRows: number;
  totalCols: number;
  truncated: boolean;
}

export interface SlidePreview {
  index: number;
  title: string | null;
  body: string[];
  notes: string[];
}

export interface PptxPreview {
  slides: SlidePreview[];
}

export const previewApi = {
  docx: (agentId: string, path: string) =>
    post<DocxPreview>("preview_docx", { agentId, path }),
  xlsx: (agentId: string, path: string, sheet?: string) =>
    post<XlsxPreview>("preview_xlsx", { agentId, path, sheet }),
  pptx: (agentId: string, path: string) =>
    post<PptxPreview>("preview_pptx", { agentId, path }),
};

export const fileApi = {
  listAgentFolder: (agentId: string, subPath?: string) =>
    post<FileEntry[]>("list_agent_folder", { agentId, subPath }),
  watchAgentFolder: (agentId: string) =>
    post<void>("watch_agent_folder", { agentId }),
  unwatchAgentFolder: () => post<void>("unwatch_agent_folder"),
  openLogsFolder: () => post<void>("open_logs_folder"),
  importFilesToAgent: (agentId: string, paths: string[]) =>
    post<string[]>("import_files_to_agent", { agentId, paths }),
  readTextFile: (agentId: string, path: string) =>
    post<TextFileContent>("read_text_file", { agentId, path }),
  writeTextFile: (
    agentId: string,
    path: string,
    content: string,
    expectedMtime: number,
  ) =>
    post<TextWriteResult>("write_text_file", {
      agentId,
      path,
      content,
      expectedMtime,
    }),
  subscribeFsChanged: (handler: () => void): Promise<UnlistenFn> =>
    subscribeWs<void>("fs-changed", handler),
  subscribeFilesDropped: (
    handler: (paths: string[]) => void,
  ): Promise<UnlistenFn> =>
    subscribeWs<string[]>("files-dropped", handler),
};

export const settingsApi = {
  get: () => post<Settings>("get_settings"),
  setDefaultProvider: (provider: string | null) =>
    post<Settings>("set_default_provider", { provider }),
  setDefaultModel: (provider: string, model: string | null) =>
    post<Settings>("set_default_model", { provider, model }),
  setFirstRunDone: () => post<Settings>("set_first_run_done"),
  availableProviders: () => post<string[]>("available_providers"),
};

export interface ValidationResult {
  ok: boolean;
  error?: string;
}

export const secretsApi = {
  setApiKey: (provider: string, value: string) =>
    post<void>("set_api_key", { provider, value }),
  hasApiKey: (provider: string) =>
    post<boolean>("has_api_key", { provider }),
  clearApiKey: (provider: string) =>
    post<void>("clear_api_key", { provider }),
  validateApiKey: (provider: string) =>
    post<ValidationResult>("validate_api_key", { provider }),
};

export const skillsApi = {
  list: () => post<Skill[]>("list_skills"),
};

export const modelsApi = {
  listCatalog: () => post<CatalogEntry[]>("list_catalog"),
  listInstalled: () => post<InstalledModel[]>("list_installed_models"),
  getHardwareInfo: () => post<HardwareInfo>("get_hardware_info"),
  downloadFromCatalog: (catalogId: string) =>
    post<void>("download_from_catalog", { catalogId }),
  downloadFromUrl: (downloadId: string, url: string, filename: string) =>
    post<void>("download_from_url", { downloadId, url, filename }),
  cancelDownload: (downloadId: string) =>
    post<void>("cancel_download", { downloadId }),
  deleteModel: (filename: string) =>
    post<void>("delete_model", { filename }),
  subscribeDownload: (
    downloadId: string,
    handler: (event: DownloadEvent) => void,
  ): Promise<UnlistenFn> =>
    subscribeWs<DownloadEvent>(
      `model:download:${sanitizeChannel(downloadId)}`,
      handler,
    ),
};

function sanitizeChannel(segment: string): string {
  return segment.replace(/[^a-zA-Z0-9\-\/:_]/g, "_");
}

export const chatApi = {
  listMessages: (agentId: string) =>
    post<ChatMessage[]>("list_messages", { agentId }),

  sendMessage: (params: {
    agentId: string;
    provider: string;
    modelId: string;
    text: string;
  }) => post<RunStarted>("send_message", params),

  cancelRun: (runId: string) => post<void>("cancel_run", { runId }),

  approveHitl: (hitlId: string) =>
    post<void>("approve_hitl", { hitlId }),
  rejectHitl: (hitlId: string, reason?: string) =>
    post<void>("reject_hitl", { hitlId, reason }),
  respondToQuestion: (questionId: string, answer: string) =>
    post<void>("respond_to_question", { questionId, answer }),

  subscribeRun: (
    runId: string,
    handler: (event: RunEvent) => void,
  ): Promise<UnlistenFn> =>
    subscribeWs<RunEvent>(`chat:run:${runId}`, handler),
};
