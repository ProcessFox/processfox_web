import { AgentSwitcher } from "@/components/agent/AgentSwitcher";
import { WorkspaceSwitcher } from "@/components/workspace/WorkspaceSwitcher";
import { ChatPane } from "@/components/chat/ChatPane";
import type { StarterPrompt } from "@/lib/starterPrompts";
import { FileTree } from "@/components/filetree/FileTree";
import { PreviewPane } from "@/components/preview/PreviewPane";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import type { PendingToolCall } from "@/hooks/useAgentChat";
import type { Agent } from "@/types/agent";
import type { Workspace } from "@/types/auth";
import type { ChatMessage, PendingHitl, PendingQuestion } from "@/types/chat";

type Props = {
  workspaces: Workspace[];
  activeWorkspace: Workspace | null;
  isOwner: boolean;
  onWorkspacesChanged: (selectId?: string) => void;
  agents: Agent[];
  activeAgent: Agent | null;
  selectedFile: { fileId: string; name: string } | null;
  messages: ChatMessage[];
  streamingText: string | null;
  streamingReasoning: string | null;
  pendingTools: PendingToolCall[];
  pendingHitl: PendingHitl | null;
  pendingQuestion: PendingQuestion | null;
  sending: boolean;
  chatError: string | null;
  chatDisabled: boolean;
  chatDisabledReason: string | undefined;
  starterPrompts: StarterPrompt[];
  inputPrefill?: { text: string; token: number };
  acceptsAttachments: string[];
  onAgentUpdated: (agent: Agent) => void;
  chatFooter?: { templateName: string | null; model: string | null };
  fileTreeRefresh: number;
  onSelectWorkspace: (workspace: Workspace) => void;
  onSelectAgent: (agent: Agent) => void;
  onCreateAgent: () => void;
  onEditAgent: () => void;
  onOpenSettings: () => void;
  onSelectFile: (fileId: string, name: string) => void;
  onClosePreview: () => void;
  onSendMessage: (text: string) => void;
  onCancelRun: () => void;
  onApproveHitl: () => void;
  onRejectHitl: () => void;
  onRespondToQuestion: (answer: string) => void;
  onPrefillInput: (text: string) => void;
  onDismissChatError: () => void;
};

export function Main({
  workspaces,
  activeWorkspace,
  agents,
  activeAgent,
  selectedFile,
  messages,
  streamingText,
  streamingReasoning,
  pendingTools,
  pendingHitl,
  pendingQuestion,
  sending,
  chatError,
  chatDisabled,
  chatDisabledReason,
  starterPrompts,
  inputPrefill,
  acceptsAttachments,
  onAgentUpdated,
  chatFooter,
  fileTreeRefresh,
  isOwner,
  onWorkspacesChanged,
  onSelectWorkspace,
  onSelectAgent,
  onCreateAgent,
  onEditAgent,
  onOpenSettings,
  onSelectFile,
  onClosePreview,
  onSendMessage,
  onCancelRun,
  onApproveHitl,
  onRejectHitl,
  onRespondToQuestion,
  onPrefillInput,
  onDismissChatError,
}: Props) {
  const showPreview = selectedFile !== null;

  return (
    <ResizablePanelGroup
      direction="horizontal"
      className="h-full w-full bg-background"
    >
      <ResizablePanel defaultSize={22} minSize={16} maxSize={36}>
        <div className="flex h-full flex-col border-r border-border bg-surface">
          <WorkspaceSwitcher
            workspaces={workspaces}
            activeWorkspace={activeWorkspace}
            isOwner={isOwner}
            onSelect={onSelectWorkspace}
            onChanged={onWorkspacesChanged}
          />
          <AgentSwitcher
            agents={agents}
            activeAgent={activeAgent}
            onSelect={onSelectAgent}
            onCreate={onCreateAgent}
            onEdit={onEditAgent}
            onOpenSettings={onOpenSettings}
          />
          <div className="flex-1 overflow-hidden border-t border-border">
            <FileTree
              workspaceId={activeWorkspace?.id ?? null}
              refreshSignal={fileTreeRefresh}
              onSelectFile={onSelectFile}
            />
          </div>
        </div>
      </ResizablePanel>

      <ResizableHandle />

      {showPreview && (
        <>
          <ResizablePanel defaultSize={38} minSize={20}>
            <PreviewPane
              fileId={selectedFile?.fileId ?? null}
              fileName={selectedFile?.name ?? null}
              onClose={onClosePreview}
            />
          </ResizablePanel>
          <ResizableHandle />
        </>
      )}

      <ResizablePanel defaultSize={showPreview ? 40 : 78} minSize={30}>
        <ChatPane
          messages={messages}
          streamingText={streamingText}
          streamingReasoning={streamingReasoning}
          pendingTools={pendingTools}
          pendingHitl={pendingHitl}
          pendingQuestion={pendingQuestion}
          sending={sending}
          error={chatError}
          disabled={chatDisabled}
          disabledReason={chatDisabledReason}
          starterPrompts={starterPrompts}
          inputPrefill={inputPrefill}
          agent={activeAgent}
          acceptsAttachments={acceptsAttachments}
          onAgentUpdated={onAgentUpdated}
          footer={chatFooter}
          onSend={onSendMessage}
          onCancel={onCancelRun}
          onApproveHitl={onApproveHitl}
          onRejectHitl={onRejectHitl}
          onRespondToQuestion={onRespondToQuestion}
          onPrefillInput={onPrefillInput}
          onDismissError={onDismissChatError}
          onOpenSettings={onOpenSettings}
        />
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
