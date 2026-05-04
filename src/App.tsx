import { useCallback, useEffect, useMemo, useState } from "react";

import { AgentEditorDialog } from "@/components/agent/AgentEditorDialog";
import { ThemeProvider } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import { resolveAgentModel, useAgentChat } from "@/hooks/useAgentChat";
import { Main } from "@/views/Main";
import { SettingsDialog } from "@/views/Settings";
import { WelcomeDialog } from "@/views/Welcome";
import {
  agentApi,
  fileApi,
  modelsApi,
  secretsApi,
  settingsApi,
  skillsApi,
} from "@/lib/tauri";
import { pickStarterPrompts } from "@/lib/starterPrompts";
import type { Agent } from "@/types/agent";
import type { CatalogEntry, InstalledModel } from "@/types/models";
import type { Settings } from "@/types/settings";
import type { Skill } from "@/types/skill";
import type { EffectiveModel } from "@/hooks/useAgentChat";

type SelectedFile = { path: string; name: string } | null;

export default function App() {
  return (
    <ThemeProvider>
      {/* 150 ms feels close to instant without firing tooltips on every
          casual mouse drift across the UI. Native `title` is ~500 ms and
          can't be tuned. */}
      <TooltipProvider delayDuration={150} skipDelayDuration={50}>
        <AppShell />
      </TooltipProvider>
    </ThemeProvider>
  );
}

function AppShell() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [activeAgent, setActiveAgent] = useState<Agent | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [hasApiKey, setHasApiKey] = useState<boolean | null>(null);
  const [installedModels, setInstalledModels] = useState<InstalledModel[]>([]);
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [selectedFile, setSelectedFile] = useState<SelectedFile>(null);
  const [fileTreeRefresh, setFileTreeRefresh] = useState(0);

  const [settingsState, setSettingsState] = useState<
    { open: false } | { open: true; tab: "models" | "cloud" | "appearance" | "about" }
  >({ open: false });
  const [agentEditor, setAgentEditor] = useState<
    { mode: "create" | "edit" } | null
  >(null);
  const [inputPrefill, setInputPrefill] = useState<
    { text: string; token: number } | undefined
  >(undefined);

  const handlePrefillInput = useCallback((text: string) => {
    setInputPrefill((prev) => ({ text, token: (prev?.token ?? 0) + 1 }));
  }, []);

  const starterPrompts = useMemo(
    () => pickStarterPrompts(activeAgent?.skills ?? []),
    [activeAgent],
  );

  // Union of `accepts_attachments` across the agent's enabled skills. Drives
  // whether the ChatInput renders an attachment button.
  const acceptsAttachments = useMemo(() => {
    if (!activeAgent) return [] as string[];
    const set = new Set<string>();
    for (const name of activeAgent.skills) {
      const skill = skills.find((s) => s.name === name);
      if (!skill) continue;
      for (const k of skill.acceptsAttachments ?? []) set.add(k);
    }
    return Array.from(set);
  }, [activeAgent, skills]);

  const effectiveModel = useMemo(
    () => resolveAgentModel(activeAgent, settings),
    [activeAgent, settings],
  );

  // Footer line inside the chat: template name (if attached) + active model.
  // The template is only meaningful while document-from-template is on; the
  // backend already auto-clears the path when the skill is removed, so a
  // present `templatePath` implies the skill is active.
  const chatFooter = useMemo(() => {
    if (!activeAgent) return undefined;
    const templatePath = activeAgent.attachments?.templatePath ?? null;
    const templateName = templatePath
      ? (templatePath.split(/[/\\]/).pop() ?? templatePath)
      : null;
    const model = displayModelName(effectiveModel, installedModels, catalog);
    if (!templateName && !model) return undefined;
    return { templateName, model };
  }, [activeAgent, effectiveModel, installedModels, catalog]);

  const chat = useAgentChat(activeAgent, effectiveModel);

  const handleSendMessage = useCallback(
    (text: string) => {
      // Bump the file-tree's refresh signal: the user often dropped new
      // files in their agent folder right before prompting about them.
      setFileTreeRefresh((n) => n + 1);
      chat.send(text);
    },
    [chat],
  );

  const refreshAgents = useCallback(async () => {
    const list = await agentApi.list();
    setAgents(list);
    return list;
  }, []);

  const refreshSettings = useCallback(async () => {
    const s = await settingsApi.get();
    setSettings(s);
    return s;
  }, []);

  useEffect(() => {
    skillsApi.list().then(setSkills).catch(console.error);
    // Catalog is small, immutable per release, and used for friendly model
    // names across the app — load once at mount and keep it.
    modelsApi.listCatalog().then(setCatalog).catch(console.error);
  }, []);

  useEffect(() => {
    Promise.all([refreshAgents(), refreshSettings()])
      .then(([list]) => {
        if (list.length > 0) setActiveAgent(list[0]);
      })
      .catch((e) => console.error("initial load failed", e));
  }, [refreshAgents, refreshSettings]);

  // Watcher fires when an agent's attachment was auto-cleared because the
  // file disappeared. Refresh the affected agent so the icon flips to warn.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    agentApi
      .subscribeAttachmentsChanged((agentId) => {
        agentApi
          .get(agentId)
          .then((updated) => {
            setAgents((prev) =>
              prev.map((a) => (a.id === updated.id ? updated : a)),
            );
            setActiveAgent((curr) =>
              curr && curr.id === updated.id ? updated : curr,
            );
          })
          .catch((e) => console.warn("agent refresh after attachment-change failed", e));
      })
      .then((u) => {
        unlisten = u;
      })
      .catch((e) => console.warn("attachment-changed subscribe failed", e));
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const showWelcome = settings !== null && !settings.firstRunDone;

  const handleFinishWelcome = useCallback(async () => {
    try {
      const updated = await settingsApi.setFirstRunDone();
      setSettings(updated);
    } catch (e) {
      console.error("set first-run-done failed", e);
    }
  }, []);

  // Refresh the installed-models list whenever Settings closes (the user may
  // have downloaded or deleted a model). This feeds the local-model gate below.
  useEffect(() => {
    if (settingsState.open) return;
    modelsApi.listInstalled().then(setInstalledModels).catch(console.error);
  }, [settingsState.open]);

  // Check API key status for the effective provider. Local models don't use
  // keychain credentials, so we short-circuit for them.
  useEffect(() => {
    if (!effectiveModel) {
      setHasApiKey(null);
      return;
    }
    if (effectiveModel.provider === "local") {
      setHasApiKey(null);
      return;
    }
    let cancelled = false;
    secretsApi
      .hasApiKey(effectiveModel.provider)
      .then((ok) => {
        if (!cancelled) setHasApiKey(ok);
      })
      .catch(() => {
        if (!cancelled) setHasApiKey(false);
      });
    return () => {
      cancelled = true;
    };
  }, [effectiveModel, settingsState.open]);

  const handleSelectAgent = useCallback((agent: Agent) => {
    setActiveAgent(agent);
    setSelectedFile(null);
  }, []);

  const handleCreateAgent = useCallback(() => {
    setAgentEditor({ mode: "create" });
  }, []);

  const handleEditAgent = useCallback(() => {
    if (!activeAgent) return;
    setAgentEditor({ mode: "edit" });
  }, [activeAgent]);

  const handleAgentSaved = useCallback(
    async (saved: Agent) => {
      await refreshAgents();
      setActiveAgent(saved);
      setSelectedFile(null);
    },
    [refreshAgents],
  );

  const handleAgentUpdated = useCallback((updated: Agent) => {
    setAgents((prev) => prev.map((a) => (a.id === updated.id ? updated : a)));
    setActiveAgent((curr) => (curr && curr.id === updated.id ? updated : curr));
  }, []);

  const handleSelectFile = useCallback((path: string, name: string) => {
    setSelectedFile({ path, name });
  }, []);

  const handleClosePreview = useCallback(() => setSelectedFile(null), []);

  const handleOpenSettings = useCallback(
    () => setSettingsState({ open: true, tab: "cloud" }),
    [],
  );

  // Global keyboard shortcuts: Cmd/Ctrl+N for new agent, Cmd/Ctrl+, for
  // settings. Cmd/Ctrl+Enter to send is handled inside ChatInput. We skip
  // the shortcut when the user is typing in an input/textarea so it doesn't
  // hijack legitimate keystrokes (e.g. , in a chat message).
  useEffect(() => {
    function handle(e: KeyboardEvent) {
      if (!(e.metaKey || e.ctrlKey)) return;
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName?.toLowerCase();
      const inField =
        tag === "input" || tag === "textarea" || target?.isContentEditable;
      if (e.key === "n" && !inField) {
        e.preventDefault();
        setAgentEditor({ mode: "create" });
      } else if (e.key === ",") {
        // Cmd+, opens settings even from inside fields — that's how every
        // macOS app behaves; users would be surprised otherwise.
        e.preventDefault();
        setSettingsState({ open: true, tab: "cloud" });
      }
    }
    window.addEventListener("keydown", handle);
    return () => window.removeEventListener("keydown", handle);
  }, []);

  // OS drag-and-drop: when the user drops files anywhere on the window,
  // copy them into the active agent's folder. The folder watcher then
  // refreshes the FileTree on its own.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    fileApi
      .subscribeFilesDropped((paths) => {
        if (!activeAgent || !activeAgent.folder) return;
        fileApi.importFilesToAgent(activeAgent.id, paths).catch((e) => {
          console.warn("import failed", e);
        });
      })
      .then((u) => {
        unlisten = u;
      })
      .catch((e) => console.warn("drop subscribe failed", e));
    return () => {
      if (unlisten) unlisten();
    };
  }, [activeAgent]);
  const handleCloseSettings = useCallback(() => {
    setSettingsState({ open: false });
    // Re-fetch settings on close in case the user changed a default.
    refreshSettings().catch(console.error);
  }, [refreshSettings]);

  const handleSettingsChange = useCallback((s: Settings) => {
    setSettings(s);
  }, []);

  // Compute chat disabled state and reason.
  const { chatDisabled, chatDisabledReason } = (() => {
    if (!activeAgent) {
      return {
        chatDisabled: true,
        chatDisabledReason: "Leg zunächst einen Agenten an." as string | undefined,
      };
    }
    if (!effectiveModel) {
      return {
        chatDisabled: true,
        chatDisabledReason:
          "Kein Modell konfiguriert — in den Einstellungen einen Default setzen oder im Agenten überschreiben.",
      };
    }
    if (effectiveModel.provider === "local") {
      const present = installedModels.some(
        (m) => m.filename === effectiveModel.modelId,
      );
      if (!present) {
        return {
          chatDisabled: true,
          chatDisabledReason: `Lokales Modell „${effectiveModel.modelId}" ist nicht installiert.`,
        };
      }
    } else if (hasApiKey === false) {
      return {
        chatDisabled: true,
        chatDisabledReason: `Kein API-Key für ${effectiveModel.provider} hinterlegt.`,
      };
    }
    return {
      chatDisabled: false,
      chatDisabledReason: undefined as string | undefined,
    };
  })();

  return (
    <div className="flex h-full w-full flex-col">
      <Main
        agents={agents}
        activeAgent={activeAgent}
        selectedFile={selectedFile}
        messages={chat.messages}
        streamingText={chat.streamingText}
        streamingReasoning={chat.streamingReasoning}
        pendingTools={chat.pendingTools}
        pendingHitl={chat.pendingHitl}
        pendingQuestion={chat.pendingQuestion}
        sending={chat.sending}
        chatError={chat.error}
        chatDisabled={chatDisabled}
        chatDisabledReason={chatDisabledReason}
        starterPrompts={starterPrompts}
        inputPrefill={inputPrefill}
        acceptsAttachments={acceptsAttachments}
        onAgentUpdated={handleAgentUpdated}
        chatFooter={chatFooter}
        fileTreeRefresh={fileTreeRefresh}
        onSelectAgent={handleSelectAgent}
        onCreateAgent={handleCreateAgent}
        onEditAgent={handleEditAgent}
        onOpenSettings={handleOpenSettings}
        onSelectFile={handleSelectFile}
        onClosePreview={handleClosePreview}
        onSendMessage={handleSendMessage}
        onCancelRun={chat.cancel}
        onApproveHitl={chat.approveHitl}
        onRejectHitl={() => chat.rejectHitl()}
        onRespondToQuestion={chat.respondToQuestion}
        onPrefillInput={handlePrefillInput}
        onDismissChatError={chat.clearError}
      />

      <AgentEditorDialog
        open={agentEditor !== null}
        mode={agentEditor?.mode ?? "create"}
        agent={agentEditor?.mode === "edit" ? activeAgent : null}
        onClose={() => setAgentEditor(null)}
        onSaved={handleAgentSaved}
      />

      <SettingsDialog
        open={settingsState.open}
        defaultTab={settingsState.open ? settingsState.tab : undefined}
        onClose={handleCloseSettings}
        onSettingsChange={handleSettingsChange}
      />

      <WelcomeDialog
        open={showWelcome && !settingsState.open && agentEditor === null}
        settings={settings}
        installedModels={installedModels}
        hasApiKey={hasApiKey}
        agents={agents}
        onOpenSettings={() =>
          setSettingsState({ open: true, tab: "models" })
        }
        onCreateAgent={() => setAgentEditor({ mode: "create" })}
        onFinish={handleFinishWelcome}
      />
    </div>
  );
}

/** Friendly label for a resolved model. Local: catalog title when known,
 *  otherwise the GGUF filename without extension. Cloud: the provider's
 *  raw model id — they're already human-readable and rotate too often to
 *  hand-curate. */
function displayModelName(
  model: EffectiveModel | null,
  installed: InstalledModel[],
  catalog: CatalogEntry[],
): string | null {
  if (!model) return null;
  if (model.provider !== "local") return model.modelId;
  const match = installed.find((m) => m.filename === model.modelId);
  if (match?.catalogId) {
    const entry = catalog.find((c) => c.id === match.catalogId);
    if (entry) return entry.title;
  }
  return model.modelId.replace(/\.gguf$/i, "");
}
