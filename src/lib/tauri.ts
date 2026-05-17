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
import type {
  AuthSession,
  Workspace,
  WorkspaceMember,
  WorkspaceRole,
} from "@/types/auth";

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

/** Authentifizierter REST-Call gegen /api/v1 mit 401→Refresh→Retry.
 *  `void` für 204-Antworten (kein JSON-Body). */
async function v1<T>(
  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
  path: string,
  body?: unknown,
): Promise<T> {
  const doFetch = () =>
    fetch(`${V1}/${path}`, {
      method,
      headers: authHeaders(
        body !== undefined ? { "Content-Type": "application/json" } : undefined,
      ),
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  let res = await doFetch();
  if (res.status === 401 && (await tryRefresh())) res = await doFetch();
  if (res.status === 401) onSessionLost?.();
  if (!res.ok) return parseError(res);
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

/** Authentifizierter Multipart-Upload gegen /api/v1 (401→Refresh→Retry). */
async function v1Upload<T>(path: string, file: File): Promise<T> {
  const build = () => {
    const form = new FormData();
    form.append("file", file);
    return fetch(`${V1}/${path}`, {
      method: "POST",
      headers: authHeaders(),
      body: form,
    });
  };
  let res = await build();
  if (res.status === 401 && (await tryRefresh())) res = await build();
  if (res.status === 401) onSessionLost?.();
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
  list: () => v1<Workspace[]>("GET", "workspaces"),
  create: (name: string) => v1<Workspace>("POST", "workspaces", { name }),
  rename: (id: string, name: string) =>
    v1<void>("PATCH", `workspaces/${id}`, { name }),
  delete: (id: string) => v1<void>("DELETE", `workspaces/${id}`),
};

export const memberApi = {
  list: (workspaceId: string) =>
    v1<WorkspaceMember[]>("GET", `workspaces/${workspaceId}/members`),
  add: (workspaceId: string, email: string, role: WorkspaceRole) =>
    v1<void>("POST", `workspaces/${workspaceId}/members`, { email, role }),
  setRole: (workspaceId: string, userId: string, role: WorkspaceRole) =>
    v1<void>("PATCH", `workspaces/${workspaceId}/members/${userId}`, { role }),
  remove: (workspaceId: string, userId: string) =>
    v1<void>("DELETE", `workspaces/${workspaceId}/members/${userId}`),
};

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

export const agentApi = {
  list: (workspaceId: string) =>
    v1<Agent[]>("GET", `workspaces/${workspaceId}/agents`),
  get: (id: string) => v1<Agent>("GET", `agents/${id}`),
  create: (draft: AgentDraft) =>
    v1<Agent>("POST", `workspaces/${draft.workspaceId}/agents`, draft),
  update: (id: string, update: AgentUpdate) =>
    v1<Agent>("PATCH", `agents/${id}`, update),
  delete: (id: string) => v1<void>("DELETE", `agents/${id}`),
  /** `fileId` referenziert eine Datei im Workspace; `null` löst die
   *  Verknüpfung. */
  setAttachment: (
    agentId: string,
    kind: AttachmentKind,
    fileId: string | null,
  ) => v1<Agent>("POST", `agents/${agentId}/attachment`, { kind, fileId }),
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
  docx: (fileId: string) =>
    v1<DocxPreview>("GET", `files/${fileId}/preview/docx`),
  xlsx: (fileId: string, sheet?: string) =>
    v1<XlsxPreview>(
      "GET",
      `files/${fileId}/preview/xlsx${
        sheet ? `?sheet=${encodeURIComponent(sheet)}` : ""
      }`,
    ),
  pptx: (fileId: string) =>
    v1<PptxPreview>("GET", `files/${fileId}/preview/pptx`),
};

// ---------------------------------------------------------------------------
// Workspace-Dateien (Upload statt OS-Ordner — CLAUDE.md §5)
// ---------------------------------------------------------------------------

export const fileApi = {
  list: (workspaceId: string) =>
    v1<WorkspaceFile[]>("GET", `workspaces/${workspaceId}/files`),
  upload: (workspaceId: string, file: File) =>
    v1Upload<WorkspaceFile>(`workspaces/${workspaceId}/files`, file),
  delete: (fileId: string) => v1<void>("DELETE", `files/${fileId}`),
  /** Pre-signed Download-URL (Gültigkeit serverseitig begrenzt). */
  downloadUrl: (fileId: string) =>
    v1<{ url: string }>("GET", `files/${fileId}/download-url`),
  readTextFile: (fileId: string) =>
    v1<TextFileContent>("GET", `files/${fileId}/text`),
  writeTextFile: (fileId: string, content: string, expectedVersion: string) =>
    v1<TextWriteResult>("PUT", `files/${fileId}/text`, {
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
  get: () => v1<Settings>("GET", "settings"),
  setDefaultProvider: (provider: string | null) =>
    v1<Settings>("PUT", "settings/provider", { value: provider }),
  setDefaultModel: (model: string | null) =>
    v1<Settings>("PUT", "settings/model", { value: model }),
  setFirstRunDone: () => v1<Settings>("POST", "settings/first-run-done"),
  // Statische Liste — kein Backend-Endpunkt nötig.
  availableProviders: (): Promise<string[]> =>
    Promise.resolve(["anthropic", "openai", "openrouter"]),
};

export interface ValidationResult {
  ok: boolean;
  error?: string;
}

/** API-Keys werden pro Organisation hinterlegt und nie ans Frontend
 *  exponiert — hier nur Status/Validierung (CLAUDE.md §10). */
export const secretsApi = {
  setApiKey: (provider: string, value: string) =>
    v1<void>("POST", `secrets/${provider}`, { value }),
  hasApiKey: (provider: string) =>
    v1<{ hasKey: boolean }>("GET", `secrets/${provider}`).then(
      (r) => r.hasKey,
    ),
  clearApiKey: (provider: string) =>
    v1<void>("DELETE", `secrets/${provider}`),
  validateApiKey: (provider: string) =>
    v1<ValidationResult>("POST", `secrets/${provider}/validate`),
};

export const skillsApi = {
  list: () => v1<Skill[]>("GET", "skills"),
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
