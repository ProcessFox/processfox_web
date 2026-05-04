import {
  AlertTriangle,
  Check,
  Eye,
  EyeOff,
  KeyRound,
  Loader2,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { secretsApi, settingsApi } from "@/lib/tauri";
import type { Settings } from "@/types/settings";

type ProviderMeta = {
  id: string;
  label: string;
  suggestedModels: string[];
  placeholder: string;
  helpUrl: string;
};

const PROVIDERS: ProviderMeta[] = [
  {
    id: "anthropic",
    label: "Anthropic (Claude)",
    suggestedModels: [
      "claude-opus-4-7",
      "claude-sonnet-4-6",
      "claude-haiku-4-5",
    ],
    placeholder: "sk-ant-…",
    helpUrl: "https://console.anthropic.com/settings/keys",
  },
  {
    id: "openai",
    label: "OpenAI (GPT)",
    suggestedModels: ["gpt-5", "gpt-4o", "gpt-4o-mini"],
    placeholder: "sk-…",
    helpUrl: "https://platform.openai.com/api-keys",
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    suggestedModels: [
      "anthropic/claude-sonnet-4-6",
      "openai/gpt-4o",
      "meta-llama/llama-3.3-70b-instruct",
    ],
    placeholder: "sk-or-…",
    helpUrl: "https://openrouter.ai/settings/keys",
  },
];

type Props = {
  settings: Settings | null;
  onSettingsChange: (s: Settings) => void;
};

export function CloudApisTab({ settings, onSettingsChange }: Props) {
  return (
    <div className="flex flex-col gap-6 py-2">
      {PROVIDERS.map((p) => (
        <ProviderCard
          key={p.id}
          meta={p}
          settings={settings}
          onSettingsChange={onSettingsChange}
        />
      ))}
    </div>
  );
}

type KeyStatus =
  | { state: "unknown" }
  | { state: "none" }
  | { state: "stored" }
  | { state: "validating" }
  | { state: "valid" }
  | { state: "invalid"; message: string };

function ProviderCard({
  meta,
  settings,
  onSettingsChange,
}: {
  meta: ProviderMeta;
  settings: Settings | null;
  onSettingsChange: (s: Settings) => void;
}) {
  const [status, setStatus] = useState<KeyStatus>({ state: "unknown" });
  const [keyInput, setKeyInput] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const defaultModel = settings?.defaultModels?.[meta.id] ?? "";

  const validate = useCallback(async () => {
    setStatus({ state: "validating" });
    try {
      const result = await secretsApi.validateApiKey(meta.id);
      if (result.ok) {
        setStatus({ state: "valid" });
      } else {
        setStatus({
          state: "invalid",
          message: result.error ?? "Unbekannter Fehler",
        });
      }
    } catch (e) {
      setStatus({
        state: "invalid",
        message: String((e as { message?: string })?.message ?? e),
      });
    }
  }, [meta.id]);

  // Initial status check on mount / provider change.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const has = await secretsApi.hasApiKey(meta.id);
        if (cancelled) return;
        if (!has) {
          setStatus({ state: "none" });
        } else {
          setStatus({ state: "stored" });
          // Kick off a background validation without blocking the UI.
          const result = await secretsApi.validateApiKey(meta.id);
          if (cancelled) return;
          if (result.ok) {
            setStatus({ state: "valid" });
            // Auto-heal: if the key is valid but the app doesn't yet have
            // a default provider/model wired up (e.g. the key was saved
            // in a previous session before this logic existed), fix it now.
            await ensureDefaultsOnFirstSetup();
          } else {
            setStatus({
              state: "invalid",
              message: result.error ?? "Unbekannter Fehler",
            });
          }
        }
      } catch (e) {
        if (!cancelled) {
          setStatus({
            state: "invalid",
            message: String((e as { message?: string })?.message ?? e),
          });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meta.id]);

  async function ensureDefaultsOnFirstSetup() {
    // If the user hasn't picked a default provider yet, make this one the
    // default so the chat is immediately usable.
    if (!settings?.defaultProvider) {
      const updated = await settingsApi.setDefaultProvider(meta.id);
      onSettingsChange(updated);
    }
    // If no default model is set for this provider yet, pick the first
    // suggestion as a sensible starter.
    if (!settings?.defaultModels?.[meta.id]) {
      const updated = await settingsApi.setDefaultModel(
        meta.id,
        meta.suggestedModels[0],
      );
      onSettingsChange(updated);
    }
    // A validated cloud provider also satisfies the first-run requirement.
    if (settings && !settings.firstRunDone) {
      const next = await settingsApi.setFirstRunDone();
      onSettingsChange(next);
    }
  }

  async function saveKey() {
    if (keyInput.trim().length === 0) return;
    setBusy(true);
    setError(null);
    try {
      await secretsApi.setApiKey(meta.id, keyInput.trim());
      setKeyInput("");
      await validate();
      await ensureDefaultsOnFirstSetup();
    } catch (e) {
      setError(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(false);
    }
  }

  async function clearKey() {
    setBusy(true);
    setError(null);
    try {
      await secretsApi.clearApiKey(meta.id);
      setStatus({ state: "none" });
    } catch (e) {
      setError(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(false);
    }
  }

  async function updateDefaultModel(model: string) {
    setBusy(true);
    try {
      const next = await settingsApi.setDefaultModel(meta.id, model || null);
      onSettingsChange(next);
      // Whenever the user actively picks a model for a provider, make that
      // provider the default if none is set yet.
      if (!settings?.defaultProvider) {
        const withProvider = await settingsApi.setDefaultProvider(meta.id);
        onSettingsChange(withProvider);
      }
    } finally {
      setBusy(false);
    }
  }

  const isActiveDefault = settings?.defaultProvider === meta.id;

  return (
    <div className="rounded-md border border-border bg-surface p-4">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <KeyRound className="h-3.5 w-3.5 text-muted-foreground" />
          <div className="text-sm font-medium">{meta.label}</div>
          {isActiveDefault && (
            <span className="rounded-sm border border-border bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
              Default
            </span>
          )}
        </div>
        <StatusBadge status={status} onRevalidate={validate} />
      </div>

      <div className="mt-3 flex flex-col gap-3">
        <div className="flex flex-col gap-1.5">
          <Label className="text-xs">API-Key</Label>
          <div className="flex items-center gap-2">
            <div className="relative flex-1">
              <Input
                type={showKey ? "text" : "password"}
                value={keyInput}
                onChange={(e) => setKeyInput(e.target.value)}
                placeholder={
                  status.state === "none"
                    ? meta.placeholder
                    : "Bestehenden Key ersetzen …"
                }
                className="pr-9"
              />
              <button
                type="button"
                onClick={() => setShowKey((v) => !v)}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                title={showKey ? "Verbergen" : "Anzeigen"}
              >
                {showKey ? (
                  <EyeOff className="h-3.5 w-3.5" />
                ) : (
                  <Eye className="h-3.5 w-3.5" />
                )}
              </button>
            </div>
            <Button
              size="sm"
              onClick={saveKey}
              disabled={busy || keyInput.trim().length === 0}
            >
              Speichern & prüfen
            </Button>
            {status.state !== "none" && status.state !== "unknown" && (
              <Button
                size="icon"
                variant="ghost"
                onClick={clearKey}
                disabled={busy}
                title="Key entfernen"
                className="h-8 w-8"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            )}
          </div>
          <a
            href={meta.helpUrl}
            target="_blank"
            rel="noreferrer"
            className="text-xs text-muted-foreground hover:text-foreground"
          >
            Wo finde ich meinen API-Key?
          </a>
        </div>

        <div className="flex flex-col gap-1.5">
          <Label className="text-xs">Default-Modell</Label>
          <div className="flex flex-wrap gap-1.5">
            {meta.suggestedModels.map((m) => (
              <button
                key={m}
                onClick={() => updateDefaultModel(m)}
                disabled={busy}
                className={`rounded-md border px-2 py-1 text-xs transition-colors ${
                  defaultModel === m
                    ? "border-primary bg-primary/10 text-foreground"
                    : "border-border bg-background text-muted-foreground hover:bg-accent"
                }`}
              >
                {m}
              </button>
            ))}
          </div>
          <Input
            value={defaultModel}
            onChange={(e) => updateDefaultModel(e.target.value)}
            placeholder="Oder custom Modell-ID eingeben …"
            className="text-xs"
          />
        </div>

        {status.state === "invalid" && (
          <div className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/15 px-3 py-2 text-xs text-destructive">
            <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <div className="flex-1">{status.message}</div>
          </div>
        )}

        {error && (
          <div className="rounded-md border border-destructive/40 bg-destructive/15 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}

function StatusBadge({
  status,
  onRevalidate,
}: {
  status: KeyStatus;
  onRevalidate: () => void;
}) {
  switch (status.state) {
    case "unknown":
      return <span className="text-xs text-muted-foreground">…</span>;
    case "none":
      return <span className="text-xs text-muted-foreground">Kein Key</span>;
    case "stored":
      return (
        <span className="flex items-center gap-1 text-xs text-muted-foreground">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          Prüfe …
        </span>
      );
    case "validating":
      return (
        <span className="flex items-center gap-1 text-xs text-muted-foreground">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          Prüfe …
        </span>
      );
    case "valid":
      return (
        <button
          onClick={onRevalidate}
          className="flex items-center gap-1 text-xs text-emerald-500 hover:underline"
          title="Erneut prüfen"
        >
          <Check className="h-3.5 w-3.5" />
          Validiert
        </button>
      );
    case "invalid":
      return (
        <button
          onClick={onRevalidate}
          className="flex items-center gap-1 text-xs text-destructive hover:underline"
          title="Erneut prüfen"
        >
          <AlertTriangle className="h-3.5 w-3.5" />
          Ungültig
        </button>
      );
  }
}
