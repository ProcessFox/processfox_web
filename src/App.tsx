import { useCallback, useEffect, useMemo, useState } from "react";

import { AgentEditorDialog } from "@/components/agent/AgentEditorDialog";
import { ThemeProvider } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import { resolveAgentModel, useAgentChat } from "@/hooks/useAgentChat";
import { useAuth } from "@/hooks/useAuth";
import { Login } from "@/views/Login";
import { Main } from "@/views/Main";
import { SettingsDialog } from "@/views/Settings";
import { WelcomeDialog } from "@/views/Welcome";
import {
  agentApi,
  secretsApi,
  settingsApi,
  skillsApi,
  workspaceApi,
} from "@/lib/tauri";
import { pickStarterPrompts } from "@/lib/starterPrompts";
import type { Agent } from "@/types/agent";
import type { Workspace } from "@/types/auth";
import type { Settings } from "@/types/settings";
import type { Skill } from "@/types/skill";

type SelectedFile = { fileId: string; name: string } | null;

export default function App() {
  return (
    <ThemeProvider>
      {/* 150 ms feels close to instant without firing tooltips on every
          casual mouse drift across the UI. Native `title` is ~500 ms and
          can't be tuned. */}
      <TooltipProvider delayDuration={150} skipDelayDuration={50}>
        <AuthGate />
      </TooltipProvider>
    </ThemeProvider>
  );
}

/** Gate: lädt die App erst nach erfolgreicher (passwordless) Anmeldung. */
function AuthGate() {
  const auth = useAuth();

  if (auth.loading) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-background text-sm text-muted-foreground">
        Lädt …
      </div>
    );
  }
  if (!auth.isAuthenticated) {
    return (
      <Login
        onRequestLogin={auth.requestLogin}
        onRequestRegister={auth.requestRegister}
      />
    );
  }
  return <AppShell onLogout={auth.logout} isOwner={auth.user!.orgRole === "owner"} />;
}

function AppShell({
  onLogout,
  isOwner,
}: {
  onLogout: () => void;
  isOwner: boolean;
}) {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [activeWorkspace, setActiveWorkspace] = useState<Workspace | null>(null);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [activeAgent, setActiveAgent] = useState<Agent | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [hasApiKey, setHasApiKey] = useState<boolean | null>(null);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [selectedFile, setSelectedFile] = useState<SelectedFile>(null);
  const [fileTreeRefresh, setFileTreeRefresh] = useState(0);

  const [settingsState, setSettingsState] = useState<
    { open: false } | { open: true; tab: "cloud" | "appearance" | "about" }
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

  // Footer line inside the chat: template indicator (if attached) + model.
  const chatFooter = useMemo(() => {
    if (!activeAgent) return undefined;
    const hasTemplate = Boolean(activeAgent.attachments?.templateFileId);
    const model = effectiveModel?.modelId ?? null;
    if (!hasTemplate && !model) return undefined;
    return { templateName: hasTemplate ? "gesetzt" : null, model };
  }, [activeAgent, effectiveModel]);

  const chat = useAgentChat(activeAgent, effectiveModel);

  const handleSendMessage = useCallback(
    (text: string) => {
      // Bump the file-tree's refresh signal: the user often uploaded new
      // files right before prompting about them.
      setFileTreeRefresh((n) => n + 1);
      chat.send(text);
    },
    [chat],
  );

  const refreshAgents = useCallback(async (workspaceId: string) => {
    const list = await agentApi.list(workspaceId);
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
  }, []);

  // Workspace-Liste laden; optional einen bestimmten (neuen) auswählen,
  // sonst Auswahl beibehalten bzw. auf den ersten fallen.
  const refreshWorkspaces = useCallback(
    async (selectId?: string) => {
      const ws = await workspaceApi.list();
      setWorkspaces(ws);
      setActiveWorkspace((curr) => {
        if (selectId) return ws.find((w) => w.id === selectId) ?? curr;
        if (curr) return ws.find((w) => w.id === curr.id) ?? ws[0] ?? null;
        return ws[0] ?? null;
      });
      return ws;
    },
    [],
  );

  // Initial load. Workspaces und Settings unabhängig — fehlt der (erst ab
  // Phase 4 implementierte) Settings-Endpunkt, blockiert das die
  // Workspace-Liste nicht.
  useEffect(() => {
    refreshWorkspaces().catch((e) =>
      console.error("workspace load failed", e),
    );
    refreshSettings().catch((e) => console.warn("settings load failed", e));
  }, [refreshWorkspaces, refreshSettings]);

  // Load agents whenever the active workspace changes.
  useEffect(() => {
    if (!activeWorkspace) {
      setAgents([]);
      setActiveAgent(null);
      return;
    }
    refreshAgents(activeWorkspace.id)
      .then((list) => {
        setActiveAgent(list.length > 0 ? list[0] : null);
        setSelectedFile(null);
      })
      .catch((e) => console.error("agent load failed", e));
  }, [activeWorkspace, refreshAgents]);

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

  // Check API key status for the effective provider.
  useEffect(() => {
    if (!effectiveModel) {
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

  const handleSelectWorkspace = useCallback((ws: Workspace) => {
    setActiveWorkspace(ws);
  }, []);

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
      if (activeWorkspace) await refreshAgents(activeWorkspace.id);
      setActiveAgent(saved);
      setSelectedFile(null);
    },
    [activeWorkspace, refreshAgents],
  );

  const handleAgentUpdated = useCallback((updated: Agent) => {
    setAgents((prev) => prev.map((a) => (a.id === updated.id ? updated : a)));
    setActiveAgent((curr) => (curr && curr.id === updated.id ? updated : curr));
  }, []);

  const handleSelectFile = useCallback((fileId: string, name: string) => {
    setSelectedFile({ fileId, name });
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
    if (hasApiKey === false) {
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
        workspaces={workspaces}
        activeWorkspace={activeWorkspace}
        isOwner={isOwner}
        onWorkspacesChanged={refreshWorkspaces}
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
        onSelectWorkspace={handleSelectWorkspace}
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
        workspaceId={activeWorkspace?.id ?? null}
        onClose={() => setAgentEditor(null)}
        onSaved={handleAgentSaved}
      />

      <SettingsDialog
        open={settingsState.open}
        defaultTab={settingsState.open ? settingsState.tab : undefined}
        onClose={handleCloseSettings}
        onSettingsChange={handleSettingsChange}
        onLogout={onLogout}
      />

      <WelcomeDialog
        open={showWelcome && !settingsState.open && agentEditor === null}
        settings={settings}
        hasApiKey={hasApiKey}
        agents={agents}
        onOpenSettings={() => setSettingsState({ open: true, tab: "cloud" })}
        onCreateAgent={() => setAgentEditor({ mode: "create" })}
        onFinish={handleFinishWelcome}
      />
    </div>
  );
}
