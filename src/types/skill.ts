export interface SkillHitl {
  default?: boolean;
  perTool?: Record<string, boolean>;
}

export interface Skill {
  name: string;
  title: string;
  description: string;
  icon?: string;
  tools: string[];
  hitl: SkillHitl;
  language: string;
  body: string;
  acceptsAttachments: string[];
}
