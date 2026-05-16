/** Cloud-only Modellreferenz. Kein lokales GGUF in Web (CLAUDE.md §10). */
export interface ModelRef {
  provider: string;
  id: string;
}

export interface SkillSetting {
  hitl?: boolean;
}

export interface AgentAttachments {
  /** ID einer Datei im Workspace (workspace_files), nicht mehr ein OS-Pfad.
   *  Wird serverseitig auto-geleert, wenn die Datei gelöscht wird. */
  templateFileId?: string | null;
}

export type AttachmentKind = "template";

export interface DelegationProfile {
  enabled: boolean;
  systemPromptOverride?: string | null;
  modelOverride?: ModelRef | null;
}

export interface Agent {
  id: string;
  name: string;
  icon: string;
  /** Der Workspace, zu dem dieser Agent gehört (ersetzt den OS-Ordner). */
  workspaceId: string;
  systemPrompt: string;
  model: ModelRef | null;
  skills: string[];
  skillSettings: Record<string, SkillSetting>;
  hitlDisabled: boolean;
  attachments: AgentAttachments;
  delegationProfile: DelegationProfile | null;
  createdAt: string;
  updatedAt: string;
}

export interface AgentDraft {
  name: string;
  icon?: string;
  workspaceId: string;
  systemPrompt?: string;
  model?: ModelRef;
  skills?: string[];
  hitlDisabled?: boolean;
  delegationProfile?: DelegationProfile;
}

export interface AgentUpdate {
  name?: string;
  icon?: string;
  systemPrompt?: string;
  model?: ModelRef;
  skills?: string[];
  hitlDisabled?: boolean;
  delegationProfile?: DelegationProfile;
}
