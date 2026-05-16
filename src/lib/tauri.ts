/**
 * Web API bridge — Ersatz für die Tauri invoke/listen-Bridge.
 *
 * Transport ist aktuell RPC-Stil (`POST /api/<command>`). Die Umstellung
 * auf das REST-Schema `/api/v1/...` + Auth-Header ist als spätere Etappe
 * geplant (siehe PLAN.md, CLAUDE.md §7) — die Funktions-Signaturen hier
 * sind bereits auf das Web-Paradigma (Workspace + Upload) ausgelegt, die
 * Backend-Implementierung folgt in den Phasen 1–6.
 */

import type {
  Agent,
  AgentDraft,
  AgentUpdate,
  AttachmentKind,
} from "@/types/agent";
import type { ChatMessage, RunEvent, RunStarted } from "@/types/chat";
import type { WorkspaceFile } from "@/types/file";
import type { Settings } from "@/types/settings";
import type { Skill } from "@/types/skill";
import type { AuthSession, Workspace } from "@/types/auth";

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

const BASE = "/api";
const V1 = "/api/v1";

// In-Memory-Access-Token (kein localStorage — XSS-Härtung). Wird von
// useAuth gesetzt; bei Reload via Refresh-Cookie wiederhergestellt.
let accessToken: string | null = null;
export function setAccessToken(token: string | null): void {
  accessToken = token;
}
/** Wird von useAuth gesetzt, damit ein 401 hier transparent refreshen kann. */
let onSessionRefreshed: ((s: AuthSession) => void) | null = null;
let onSessionLost: (() => void) | null = null;
export function setAuthCallbacks(
  refreshed: (s: AuthSession) => void,
  lost: () => void,
): void {
  onSessionRefreshed = refreshed;
  onSessionLost = lost;
}

function authHeaders(extra?: Record<string, string>): Record<string, string> {
  const h: Record<string, string> = { ...(extra ?? {}) };
  if (accessToken) h.Authorization = `Bearer ${accessToken}`;
  return h;
}

// Single-Flight-Refresh: parallele 401er teilen sich einen Refresh-Call.
let refreshInflight: Promise<boolean> | null = null;
async function tryRefresh(): Promise<boolean> {
  if (!refreshInflight) {
    refreshInflight = fetch(`${V1}/auth/refresh`, { method: "POST" })
      .then(async (res) => {
        if (!res.ok) return false;
        const session = (await res.json()) as AuthSession;
        accessToken = session.accessToken;
        onSessionRefreshed?.(session);
        return true;
      })
      .catch(() => false)
      .finally(() => {
        refreshInflight = null;
      });
  }
  return refreshInflight;
}

async function parseError(res: Response): Promise<never> {
  const err = await res.json().catch(() => ({ message: res.statusText }));
  throw new Error((err as { message?: string }).message ?? res.statusText);
}

async function post<T>(command: string, body?: unknown): Promise<T> {
  const doFetch = () =>
    fetch(`${BASE}/${command}`, {
      method: "POST",
      headers: authHeaders({ "Content-Type": "application/json" }),
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  let res = await doFetch();
  if (res.status === 401 && (await tryRefresh())) {
    res = await doFetch();
  }
  if (res.status === 401) onSessionLost?.();
  if (!res.ok) return parseError(res);
  return res.json() as Promise<T>;
}

async function upload<T>(
  command: string,
  fields: Record<string, string>,
  file: File,
): Promise<T> {
  const build = () => {
    const form = new FormData();
    for (const [k, v] of Object.entries(fields)) form.append(k, v);
    form.append("file", file);
    return fetch(`${BASE}/${command}`, {
      method: "POST",
      headers: authHeaders(),
      body: form,
    });
  };
  let res = await build();
  if (res.status === 401 && (await tryRefresh())) {
    res = await build();
  }
  if (res.status === 401) onSessionLost?.();
  if (!res.ok) return parseError(res);
  return res.json() as Promise<T>;
}

// --- Auth (REST, /api/v1/auth/*) ------------------------------------------

export const authApi = {
  requestLogin: (email: string) =>
    v1Post<{ ok: boolean; message: string }>("auth/request-login", { email }),
  requestRegister: (email: string, inviteCode: string) =>
    v1Post<{ ok: boolean; message: string }>("auth/request-register", {
      email,
      inviteCode,
    }),
  verify: (token: string) => v1Post<AuthSession>("auth/verify", { token }),
  /** Stellt eine Session aus dem httpOnly-Refresh-Cookie wieder her. */
  refresh: async (): Promise<AuthSession | null> => {
    const res = await fetch(`${V1}/auth/refresh`, { method: "POST" });
    if (!res.ok) return null;
    const s = (await res.json()) as AuthSession;
    accessToken = s.accessToken;
    return s;
  },
  logout: async (): Promise<void> => {
    await fetch(`${V1}/auth/logout`, {
      method: "POST",
      headers: authHeaders(),
    }).catch(() => {});
    accessToken = null;
  },
};

async function v1Post<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${V1}/${path}`, {
    method: "POST",
    headers: authHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify(body),
  });
  if (!res.ok) return parseError(res);
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
// Workspaces
// ---------------------------------------------------------------------------

export const workspaceApi = {
  list: () => post<Workspace[]>("list_workspaces"),
};

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

export const agentApi = {
  list: (workspaceId: string) =>
    post<Agent[]>("list_agents", { workspaceId }),
  get: (id: string) => post<Agent>("get_agent", { id }),
  create: (draft: AgentDraft) => post<Agent>("create_agent", { draft }),
  update: (id: string, update: AgentUpdate) =>
    post<Agent>("update_agent", { id, update }),
  delete: (id: string) => post<void>("delete_agent", { id }),
  /** `fileId` referenziert eine Datei im Workspace; `null` löst die
   *  Verknüpfung. */
  setAttachment: (
    agentId: string,
    kind: AttachmentKind,
    fileId: string | null,
  ) => post<Agent>("set_agent_attachment", { agentId, kind, fileId }),
  subscribeAttachmentsChanged: (
    handler: (agentId: string) => void,
  ): Promise<UnlistenFn> =>
    subscribeWs<string>("agent-attachments-changed", handler),
};

// ---------------------------------------------------------------------------
// Preview (serverseitig aus S3 geladen — gleiches JSON wie in Local)
// ---------------------------------------------------------------------------

export interface TextFileContent {
  content: string;
  /** Versions-Token (z. B. S3-ETag) — beim Speichern unverändert
   *  zurückgeben, damit der Server externe Änderungen erkennt. */
  version: string;
}

export interface TextWriteResult {
  version: string;
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
  docx: (fileId: string) => post<DocxPreview>("preview_docx", { fileId }),
  xlsx: (fileId: string, sheet?: string) =>
    post<XlsxPreview>("preview_xlsx", { fileId, sheet }),
  pptx: (fileId: string) => post<PptxPreview>("preview_pptx", { fileId }),
};

// ---------------------------------------------------------------------------
// Workspace-Dateien (Upload statt OS-Ordner — CLAUDE.md §5)
// ---------------------------------------------------------------------------

export const fileApi = {
  list: (workspaceId: string) =>
    post<WorkspaceFile[]>("list_files", { workspaceId }),
  upload: (workspaceId: string, file: File) =>
    upload<WorkspaceFile>("upload_file", { workspaceId }, file),
  delete: (fileId: string) => post<void>("delete_file", { fileId }),
  /** Pre-signed Download-URL (Gültigkeit serverseitig begrenzt). */
  downloadUrl: (fileId: string) =>
    post<{ url: string }>("file_download_url", { fileId }),
  readTextFile: (fileId: string) =>
    post<TextFileContent>("read_text_file", { fileId }),
  writeTextFile: (fileId: string, content: string, expectedVersion: string) =>
    post<TextWriteResult>("write_text_file", {
      fileId,
      content,
      expectedVersion,
    }),
  /** Broadcast an alle Workspace-Mitglieder bei Datei-Änderungen. */
  subscribeFsChanged: (handler: () => void): Promise<UnlistenFn> =>
    subscribeWs<void>("fs-changed", handler),
};

// ---------------------------------------------------------------------------
// Settings (pro Organisation)
// ---------------------------------------------------------------------------

export const settingsApi = {
  get: () => post<Settings>("get_settings"),
  setDefaultProvider: (provider: string | null) =>
    post<Settings>("set_default_provider", { provider }),
  setDefaultModel: (model: string | null) =>
    post<Settings>("set_default_model", { model }),
  setFirstRunDone: () => post<Settings>("set_first_run_done"),
  availableProviders: () => post<string[]>("available_providers"),
};

export interface ValidationResult {
  ok: boolean;
  error?: string;
}

/** API-Keys werden pro Organisation hinterlegt und nie ans Frontend
 *  exponiert — hier nur Status/Validierung (CLAUDE.md §10). */
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

// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

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
