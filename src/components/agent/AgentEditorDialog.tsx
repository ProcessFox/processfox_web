import { Wrench } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { DynamicIcon } from "@/components/ui/DynamicIcon";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { agentApi, settingsApi, skillsApi } from "@/lib/tauri";
import type { Agent, DelegationProfile, ModelRef } from "@/types/agent";
import type { Settings } from "@/types/settings";
import type { Skill } from "@/types/skill";

type Props = {
  open: boolean;
  mode: "create" | "edit";
  agent: Agent | null;
  /** Workspace the new agent belongs to (required when mode === "create"). */
  workspaceId: string | null;
  onClose: () => void;
  onSaved: (agent: Agent) => void;
};

type ModelSelection =
  | { kind: "inherit" }
  | { kind: "override"; provider: string; modelId: string };

const PROVIDER_OPTIONS: { value: string; label: string }[] = [
  { value: "anthropic", label: "Anthropic" },
  { value: "openai", label: "OpenAI" },
  { value: "openrouter", label: "OpenRouter" },
];

function modelRefToSelection(m: ModelRef | null | undefined): ModelSelection {
  if (!m) return { kind: "inherit" };
  return { kind: "override", provider: m.provider, modelId: m.id };
}

function selectionToModelRef(sel: ModelSelection): ModelRef | undefined {
  if (sel.kind === "inherit") return undefined;
  return { provider: sel.provider, id: sel.modelId };
}

type ModelPickerProps = {
  selection: ModelSelection;
  onChange: (s: ModelSelection) => void;
  defaultLabel: string;
  defaultHint: string;
  overrideHint: string;
};

function ModelOverridePicker({
  selection,
  onChange,
  defaultLabel,
  defaultHint,
  overrideHint,
}: ModelPickerProps) {
  return (
    <>
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => onChange({ kind: "inherit" })}
          className={`flex-1 rounded-md border px-3 py-2 text-left text-xs transition-colors ${
            selection.kind === "inherit"
              ? "border-primary bg-primary/10"
              : "border-border bg-background hover:bg-accent"
          }`}
        >
          <div className="font-medium">{defaultLabel}</div>
          <div className="mt-0.5 text-muted-foreground">{defaultHint}</div>
        </button>
        <button
          type="button"
          onClick={() =>
            onChange({
              kind: "override",
              provider:
                selection.kind === "override" ? selection.provider : "anthropic",
              modelId:
                selection.kind === "override" ? selection.modelId : "",
            })
          }
          className={`flex-1 rounded-md border px-3 py-2 text-left text-xs transition-colors ${
            selection.kind === "override"
              ? "border-primary bg-primary/10"
              : "border-border bg-background hover:bg-accent"
          }`}
        >
          <div className="font-medium">Override</div>
          <div className="mt-0.5 text-muted-foreground">{overrideHint}</div>
        </button>
      </div>

      {selection.kind === "override" && (
        <div className="mt-2 grid grid-cols-[140px_1fr] gap-2">
          <select
            value={selection.provider}
            onChange={(e) =>
              onChange({ ...selection, provider: e.target.value })
            }
            className="h-8 rounded-md border border-border bg-background px-2 text-xs"
          >
            {PROVIDER_OPTIONS.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </select>
          <Input
            value={selection.modelId}
            onChange={(e) =>
              onChange({ ...selection, modelId: e.target.value })
            }
            placeholder="z. B. claude-sonnet-4-6"
            className="text-xs"
          />
        </div>
      )}
    </>
  );
}

export function AgentEditorDialog({
  open: isOpen,
  mode,
  agent,
  workspaceId,
  onClose,
  onSaved,
}: Props) {
  const [name, setName] = useState("");
  const [icon, setIcon] = useState("Bot");
  const [systemPrompt, setSystemPrompt] = useState("");
  const [selection, setSelection] = useState<ModelSelection>({ kind: "inherit" });
  const [settings, setSettings] = useState<Settings | null>(null);
  const [availableSkills, setAvailableSkills] = useState<Skill[]>([]);
  const [activeSkills, setActiveSkills] = useState<string[]>([]);
  const [hitlDisabled, setHitlDisabled] = useState(false);
  const [reasoningEnabled, setReasoningEnabled] = useState(false);
  const [delegationEnabled, setDelegationEnabled] = useState(false);
  const [delegationSystemPrompt, setDelegationSystemPrompt] = useState("");
  const [delegationSelection, setDelegationSelection] = useState<ModelSelection>(
    { kind: "inherit" },
  );
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    settingsApi.get().then(setSettings).catch(console.error);
    skillsApi
      .list()
      .then((list) => {
        setAvailableSkills(list);
        // New agents start with every available skill enabled — the
        // sidebar no longer surfaces per-skill toggles, so the default
        // needs to be "everything works out of the box".
        if (mode === "create") {
          setActiveSkills(list.map((s) => s.name));
        }
      })
      .catch(console.error);
    if (mode === "edit" && agent) {
      setName(agent.name);
      setIcon(agent.icon);
      setSystemPrompt(agent.systemPrompt);
      setSelection(modelRefToSelection(agent.model));
      setActiveSkills(agent.skills);
      setHitlDisabled(agent.hitlDisabled);
      setReasoningEnabled(agent.reasoningEnabled);
      const dp = agent.delegationProfile;
      setDelegationEnabled(dp?.enabled ?? false);
      setDelegationSystemPrompt(dp?.systemPromptOverride ?? "");
      setDelegationSelection(modelRefToSelection(dp?.modelOverride ?? null));
    } else {
      setName("");
      setIcon("Bot");
      setSystemPrompt("");
      setSelection({ kind: "inherit" });
      setActiveSkills([]);
      setHitlDisabled(false);
      setReasoningEnabled(false);
      setDelegationEnabled(false);
      setDelegationSystemPrompt("");
      setDelegationSelection({ kind: "inherit" });
    }
    setError(null);
  }, [isOpen, mode, agent]);

  async function handleSave() {
    if (
      delegationEnabled &&
      delegationSelection.kind === "override" &&
      delegationSelection.modelId.trim().length === 0
    ) {
      setError(
        "Bitte ein Modell für den Hintergrund-Worker wählen oder auf Default zurücksetzen.",
      );
      return;
    }
    if (mode === "create" && !workspaceId) {
      setError("Kein Workspace ausgewählt.");
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      const model = selectionToModelRef(selection);
      const trimmedDelegationPrompt = delegationSystemPrompt.trim();
      const delegationProfile: DelegationProfile = {
        enabled: delegationEnabled,
        systemPromptOverride: trimmedDelegationPrompt
          ? delegationSystemPrompt
          : null,
        modelOverride: selectionToModelRef(delegationSelection) ?? null,
      };
      const saved =
        mode === "create"
          ? await agentApi.create({
              name: name.trim(),
              icon,
              workspaceId: workspaceId!,
              systemPrompt,
              model,
              skills: activeSkills,
              hitlDisabled,
              reasoningEnabled,
              delegationProfile,
            })
          : await agentApi.update(agent!.id, {
              name: name.trim(),
              icon,
              systemPrompt,
              model,
              skills: activeSkills,
              hitlDisabled,
              reasoningEnabled,
              delegationProfile,
            });
      onSaved(saved);
      onClose();
    } catch (e) {
      const msg =
        typeof e === "object" && e && "message" in e
          ? String((e as { message: unknown }).message)
          : String(e);
      setError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  const inheritedHint = (() => {
    if (!settings) return "…";
    const provider = settings.defaultProvider;
    if (!provider) return "Kein Default gesetzt (in Einstellungen konfigurieren).";
    const model = settings.defaultModel;
    return model ? `${provider} · ${model}` : `${provider} · kein Default-Modell`;
  })();

  return (
    <Dialog open={isOpen} onOpenChange={(v) => !v && onClose()}>
      <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-[560px]">
        <DialogHeader>
          <DialogTitle>
            {mode === "create" ? "Neuer Agent" : "Agent bearbeiten"}
          </DialogTitle>
        </DialogHeader>

        <div className="-mx-1 flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-1 py-1">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="agent-name" className="text-xs">
              Name
            </Label>
            <Input
              id="agent-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="z. B. Angebots-Assistent"
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="agent-prompt" className="text-xs">
              System-Prompt
            </Label>
            <Textarea
              id="agent-prompt"
              value={systemPrompt}
              onChange={(e) => setSystemPrompt(e.target.value)}
              rows={4}
              placeholder="Beschreibe, wie der Agent antworten soll …"
              className="resize-none"
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <Label className="text-xs">Modell</Label>
            <ModelOverridePicker
              selection={selection}
              onChange={setSelection}
              defaultLabel="Default"
              defaultHint={inheritedHint}
              overrideHint="Modell für diesen Agenten festlegen"
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <Label className="text-xs">Skills</Label>
            {availableSkills.length === 0 ? (
              <div className="rounded-md border border-dashed border-border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
                Keine Skills verfügbar.
              </div>
            ) : (
              <div className="flex flex-col gap-0.5 rounded-md border border-border bg-background p-2">
                {availableSkills.map((s) => {
                  const checked = activeSkills.includes(s.name);
                  return (
                    <label
                      key={s.name}
                      title={s.description}
                      className="flex cursor-pointer items-center gap-2 rounded-sm px-1.5 py-1 hover:bg-accent/40"
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={(e) =>
                          setActiveSkills((prev) =>
                            e.target.checked
                              ? [...prev, s.name]
                              : prev.filter((n) => n !== s.name),
                          )
                        }
                      />
                      <DynamicIcon
                        name={s.icon}
                        fallback={Wrench}
                        className="h-3.5 w-3.5 text-muted-foreground"
                      />
                      <span className="text-xs font-medium">{s.title}</span>
                      <span className="ml-auto font-mono text-[10px] text-muted-foreground">
                        {s.name}
                      </span>
                    </label>
                  );
                })}
              </div>
            )}
          </div>

          <label className="flex cursor-pointer items-start gap-2 rounded-md border border-border bg-background p-2">
            <input
              type="checkbox"
              checked={hitlDisabled}
              onChange={(e) => setHitlDisabled(e.target.checked)}
              className="mt-0.5"
            />
            <div className="flex flex-col gap-0.5">
              <span className="text-xs font-medium">
                Schreiben ohne Rückfrage
              </span>
              <span className="text-xs text-muted-foreground">
                Schreib-Tools laufen sofort durch — der Freigabe-Dialog wird
                übersprungen. Vorsicht: nur für Agenten verwenden, denen du
                das eigenständige Arbeiten in „ihrem" Workspace zutraust.
              </span>
            </div>
          </label>

          <label className="flex cursor-pointer items-start gap-2 rounded-md border border-border bg-background p-2">
            <input
              type="checkbox"
              checked={reasoningEnabled}
              onChange={(e) => setReasoningEnabled(e.target.checked)}
              className="mt-0.5"
            />
            <div className="flex flex-col gap-0.5">
              <span className="text-xs font-medium">
                Reasoning-Modus aktivieren
              </span>
              <span className="text-xs text-muted-foreground">
                Schaltet Anthropic Extended Thinking bzw. OpenRouter
                Reasoning ein. Der Chip „Denken" erscheint live während
                der Antwort und bleibt im Verlauf einklappbar erhalten.
                Greift nur bei Modellen, die das Feld unterstützen
                (z. B. Claude Sonnet 4 Thinking, OpenAI o-Serie,
                DeepSeek R1) — sonst wirkungslos. Verbraucht
                zusätzliche Tokens.
              </span>
            </div>
          </label>

          <div className="flex flex-col gap-2 rounded-md border border-border bg-background p-2">
            <label className="flex cursor-pointer items-start gap-2">
              <input
                type="checkbox"
                checked={delegationEnabled}
                onChange={(e) => setDelegationEnabled(e.target.checked)}
                className="mt-0.5"
              />
              <div className="flex flex-col gap-0.5">
                <span className="text-xs font-medium">
                  Hintergrund-Worker (Beta)
                </span>
                <span className="text-xs text-muted-foreground">
                  Aktiviert Bulk-Werkzeuge, die pro Item eine fokussierte
                  Inferenz ausführen — z. B. um eine XLSX-Spalte für jede Zeile
                  zu generieren. Standardmäßig antwortet der Worker knapp und
                  ohne Formatierung; das Modell erbt vom Agenten. Beide Werte
                  können hier überschrieben werden.
                </span>
              </div>
            </label>

            {delegationEnabled && (
              <div className="ml-6 flex flex-col gap-2 border-l border-border pl-3">
                <Label
                  htmlFor="delegation-prompt"
                  className="text-xs"
                >
                  System-Prompt für den Worker
                </Label>
                <Textarea
                  id="delegation-prompt"
                  value={delegationSystemPrompt}
                  onChange={(e) => setDelegationSystemPrompt(e.target.value)}
                  rows={3}
                  placeholder="leer = Standard-Worker-Prompt (knappe, direkte Antwort, keine Markdown-Formatierung, keine Optionen-Liste)"
                  className="resize-none text-xs"
                />
                <Label className="text-xs">Modell für den Worker</Label>
                <ModelOverridePicker
                  selection={delegationSelection}
                  onChange={setDelegationSelection}
                  defaultLabel="Wie der Agent"
                  defaultHint="Erbt das Modell des Eltern-Agenten."
                  overrideHint="Eigenes Modell für den Worker"
                />
              </div>
            )}
          </div>

          {error && (
            <div className="rounded-md border border-destructive/40 bg-destructive/15 px-3 py-2 text-xs text-destructive">
              {error}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={submitting}>
            Abbrechen
          </Button>
          <Button
            onClick={handleSave}
            disabled={submitting || name.trim().length === 0}
          >
            {mode === "create" ? "Anlegen" : "Speichern"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
