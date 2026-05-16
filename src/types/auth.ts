/**
 * Mehrbenutzer-Kernmodell: Organisation → Workspace → User.
 * Siehe CLAUDE.md §4. Backend-Implementierung folgt in Phase 2/3 —
 * diese Typen sind die Vertragsgrenze zwischen Frontend und Bridge.
 */

export type OrgRole = "owner" | "member";

/** Workspace-Rolle. `null` = User ist kein Mitglied dieses Workspaces. */
export type WorkspaceRole = "editor" | "viewer";

export interface Org {
  id: string;
  name: string;
  createdAt: string;
}

export interface User {
  id: string;
  email: string;
  orgId: string;
  orgRole: OrgRole;
  createdAt: string;
}

export interface Workspace {
  id: string;
  orgId: string;
  name: string;
  /** Rolle des aktuell eingeloggten Users in diesem Workspace. */
  role: WorkspaceRole;
  createdAt: string;
}

/** Antwort von `/api/v1/auth/login` bzw. `/auth/register`. */
export interface AuthSession {
  accessToken: string;
  /** Sekunden bis zum Ablauf des Access-Tokens. */
  expiresIn: number;
  user: User;
}
