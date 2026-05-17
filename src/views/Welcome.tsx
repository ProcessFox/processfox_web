import { Bot, Check, KeyRound } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { Agent } from "@/types/agent";
import type { Settings } from "@/types/settings";

type Props = {
  open: boolean;
  settings: Settings | null;
  hasApiKey: boolean | null;
  agents: Agent[];
  onOpenSettings: () => void;
  onCreateAgent: () => void;
  onFinish: () => void;
};

/** Two-step first-run flow: configure a cloud API key → create the first
 *  agent. Each step shows a checkmark once its precondition is satisfied. */
export function WelcomeDialog({
  open,
  settings,
  hasApiKey,
  agents,
  onOpenSettings,
  onCreateAgent,
  onFinish,
}: Props) {
  const modelReady = isModelReady(settings, hasApiKey);
  const agentReady = agents.length > 0;
  const allReady = modelReady && agentReady;

  return (
    <Dialog open={open}>
      <DialogContent
        className="sm:max-w-[560px]"
        onPointerDownOutside={(e) => e.preventDefault()}
        onEscapeKeyDown={(e) => e.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <img
              src="/icon.png"
              alt="ProcessFox"
              className="h-6 w-6 rounded-md"
            />
            Willkommen bei ProcessFox
          </DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-4 py-2">
          <p className="text-sm text-muted-foreground">
            ProcessFox lässt dein Team KI-Agenten auf gemeinsam genutzten
            Dateien arbeiten. Zwei kurze Schritte und ihr könnt loslegen:
          </p>

          <Step
            number={1}
            done={modelReady}
            icon={KeyRound}
            title="Cloud-API einrichten"
            description={
              modelReady
                ? "API-Key ist hinterlegt."
                : "Hinterlege einen API-Key (Anthropic, OpenAI oder OpenRouter) und wähle ein Default-Modell."
            }
            actionLabel={modelReady ? "Ändern" : "Einstellungen öffnen"}
            onAction={onOpenSettings}
          />

          <Step
            number={2}
            done={agentReady}
            icon={Bot}
            title="Ersten Agenten anlegen"
            description={
              agentReady
                ? `${agents.length} Agent${agents.length === 1 ? "" : "en"} angelegt.`
                : "Gib deinem Agenten einen Namen und einen System-Prompt."
            }
            actionLabel={agentReady ? "Weiteren anlegen" : "Agent anlegen"}
            onAction={onCreateAgent}
            disabled={!modelReady}
          />
        </div>

        <div className="flex justify-end pt-2">
          <Button onClick={onFinish} disabled={!allReady}>
            Fertig — los geht's
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Step({
  number,
  done,
  icon: Icon,
  title,
  description,
  actionLabel,
  onAction,
  disabled,
}: {
  number: number;
  done: boolean;
  icon: typeof Bot;
  title: string;
  description: string;
  actionLabel: string;
  onAction: () => void;
  disabled?: boolean;
}) {
  return (
    <div
      className={`flex items-start gap-3 rounded-md border p-3 ${
        done
          ? "border-emerald-500/30 bg-emerald-500/5"
          : "border-border bg-background"
      }`}
    >
      <div
        className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-medium ${
          done
            ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300"
            : "bg-muted text-muted-foreground"
        }`}
      >
        {done ? <Check className="h-3.5 w-3.5" /> : number}
      </div>
      <div className="flex-1">
        <div className="flex items-center gap-1.5 text-sm font-medium">
          <Icon className="h-3.5 w-3.5 opacity-70" />
          {title}
        </div>
        <div className="mt-0.5 text-xs text-muted-foreground">{description}</div>
      </div>
      <Button
        size="sm"
        variant={done ? "ghost" : "default"}
        onClick={onAction}
        disabled={disabled}
      >
        {actionLabel}
      </Button>
    </div>
  );
}

function isModelReady(
  settings: Settings | null,
  hasApiKey: boolean | null,
): boolean {
  if (!settings?.defaultProvider) return false;
  return hasApiKey === true;
}
