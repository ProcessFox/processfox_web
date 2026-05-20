/**
 * Mehrbenutzer-Kernmodell: Organisation → Workspace → User.
 * Siehe CLAUDE.md §4.
 *
 * Rollen-Modell (Stand 2026-05-20):
 *   - `org_role = "owner"`  → **Admin**. Darf Workspaces anlegen/umbenennen/
 *     löschen, Mitglieder einladen/entfernen, Org-Settings & API-Keys
 *     pflegen, Agenten löschen.
 *   - `org_role = "member"` → **Nutzer**. Darf chatten, alle Datei-
 *     Operationen, Agenten anlegen/konfigurieren — aber nicht löschen.
 *
 * Die frühere zweite Rollen-Ebene `workspace_members.role`
 * ("editor"/"viewer") wurde abgeschafft: wer im Workspace ist, ist
 * gleichberechtigter Nutzer.
 */

export type OrgRole = "owner" | "member";

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
  createdAt: string;
}

export interface WorkspaceMember {
  userId: string;
  email: string;
}

/** Antwort von `/api/v1/auth/login` bzw. `/auth/register`. */
export interface AuthSession {
  accessToken: string;
  /** Sekunden bis zum Ablauf des Access-Tokens. */
  expiresIn: number;
  user: User;
}
