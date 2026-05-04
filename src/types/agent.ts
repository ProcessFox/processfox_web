export type ModelRef =
  | { type: "local"; id: string }
  | { type: "cloud"; provider: string; id: string };

export interface SkillSetting {
  hitl?: boolean;
}

export interface AgentAttachments {
  templatePath?: string | null;
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
  folder: string | null;
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
  folder?: string;
  systemPrompt?: string;
  model?: ModelRef;
  skills?: string[];
  hitlDisabled?: boolean;
  delegationProfile?: DelegationProfile;
}

export interface AgentUpdate {
  name?: string;
  icon?: string;
  folder?: string;
  systemPrompt?: string;
  model?: ModelRef;
  skills?: string[];
  hitlDisabled?: boolean;
  delegationProfile?: DelegationProfile;
}
