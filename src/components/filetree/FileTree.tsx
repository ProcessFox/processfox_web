import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Tree,
  type NodeApi,
  type NodeRendererProps,
  type TreeApi,
} from "react-arborist";
import { Folder, FolderOpen } from "lucide-react";

import { iconForFile } from "@/lib/fileIcons";
import { fileApi } from "@/lib/tauri";
import type { FileEntry } from "@/types/file";
import { cn } from "@/lib/utils";

type TreeNode = {
  id: string;
  name: string;
  path: string;
  isDir: boolean;
  children?: TreeNode[];
  /** True once we've fetched this dir's contents at least once. Lets us
   *  tell "directory we never opened" apart from "directory we opened
   *  and it really was empty" — without this we'd re-fetch on every
   *  toggle. */
  loaded?: boolean;
};

type Props = {
  agentId: string | null;
  /** Absolute path to the agent's folder. We pass it explicitly (not just
   *  a `hasFolder` boolean) so the tree re-fetches when the user switches
   *  the folder of an existing agent without changing its ID. */
  agentFolder: string | null;
  /** Bump to force a refetch (e.g. after a chat message is sent). */
  refreshSignal?: number;
  onSelectFile: (path: string, name: string) => void;
  onRequestPickFolder: () => void;
};

export function FileTree({
  agentId,
  agentFolder,
  refreshSignal,
  onSelectFile,
  onRequestPickFolder,
}: Props) {
  const hasFolder = Boolean(agentFolder);
  const [data, setData] = useState<TreeNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const containerRef = useRef<HTMLDivElement>(null);
  const treeRef = useRef<TreeApi<TreeNode> | null>(null);
  const [size, setSize] = useState({ width: 260, height: 480 });

  useEffect(() => {
    if (!containerRef.current) return;
    const el = containerRef.current;
    const update = () =>
      setSize({ width: el.clientWidth, height: el.clientHeight });
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const refresh = useCallback(() => {
    if (!agentId || !hasFolder) {
      setData([]);
      return;
    }
    setLoading(true);
    setError(null);
    fileApi
      .listAgentFolder(agentId)
      .then((entries: FileEntry[]) => {
        setData(entries.map(entryToNode));
      })
      .catch((err) => {
        setError(typeof err === "string" ? err : (err?.message ?? String(err)));
      })
      .finally(() => {
        setLoading(false);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId, agentFolder]);

  // Initial load, re-load on agent / folder change, and re-load whenever
  // the parent bumps `refreshSignal` (typically when a chat message is sent
  // — common moment for the file system to have changed).
  useEffect(() => {
    refresh();
  }, [refresh, refreshSignal]);

  // Lazy-load a directory's contents the first time it's opened. We update
  // the tree by walking it and replacing the matching node — the rest of
  // the tree stays untouched so other expansions don't collapse.
  const loadChildren = useCallback(
    async (node: NodeApi<TreeNode>) => {
      if (!agentId) return;
      if (!node.data.isDir || node.data.loaded) return;
      try {
        const entries = await fileApi.listAgentFolder(agentId, node.data.path);
        const children = entries.map(entryToNode);
        setData((prev) =>
          mapTreeNode(prev, node.data.id, (n) => ({
            ...n,
            children,
            loaded: true,
          })),
        );
      } catch (err) {
        setError(typeof err === "string" ? err : (err as Error).message ?? String(err));
      }
    },
    [agentId],
  );

  // Re-load whenever the window regains focus — typical cadence for users
  // who jumped to Finder to drop files in the agent folder.
  useEffect(() => {
    if (!agentId || !hasFolder) return;
    const handler = () => refresh();
    window.addEventListener("focus", handler);
    return () => window.removeEventListener("focus", handler);
  }, [agentId, hasFolder, refresh, agentFolder]);

  // Live FS watcher: arm a backend notify-watcher on the agent folder and
  // refresh whenever it pings. Drops the watch when the agent or folder
  // changes (the next mount installs the new one).
  useEffect(() => {
    if (!agentId || !hasFolder) return;
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    fileApi
      .watchAgentFolder(agentId)
      .then(() =>
        fileApi.subscribeFsChanged(() => refresh()).then((u) => {
          if (cancelled) {
            u();
          } else {
            unlisten = u;
          }
        }),
      )
      .catch((e) => console.warn("watch failed", e));

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
      fileApi.unwatchAgentFolder().catch(() => {});
    };
  }, [agentId, hasFolder, agentFolder, refresh]);

  const content = useMemo(() => {
    if (!agentId) {
      return (
        <EmptyState
          title="Kein Agent ausgewählt"
          description="Leg oben einen Agenten an."
        />
      );
    }
    if (!hasFolder) {
      return (
        <EmptyState
          title="Kein Ordner gewählt"
          description="Verknüpfe einen Ordner mit diesem Agenten."
          action={{ label: "Ordner wählen", onClick: onRequestPickFolder }}
        />
      );
    }
    if (loading) {
      return (
        <div className="px-3 py-2 text-xs text-muted-foreground">Lädt …</div>
      );
    }
    if (error) {
      return (
        <div className="px-3 py-2 text-xs text-destructive">
          Fehler: {error}
        </div>
      );
    }
    if (data.length === 0) {
      return (
        <div className="px-3 py-2 text-xs text-muted-foreground">
          Der Ordner ist leer.
        </div>
      );
    }
    return (
      <Tree<TreeNode>
        data={data}
        width={size.width}
        height={size.height}
        rowHeight={26}
        indent={14}
        openByDefault={false}
        disableMultiSelection
        onActivate={(node: NodeApi<TreeNode>) => {
          if (!node.data.isDir) onSelectFile(node.data.path, node.data.name);
        }}
        onToggle={(id) => {
          const node = treeRef.current?.get(id);
          // react-arborist toggles BEFORE it calls onToggle, so a now-open
          // directory is what we want to lazy-load.
          if (node && node.isOpen) loadChildren(node);
        }}
        ref={treeRef}
      >
        {Node}
      </Tree>
    );
  }, [
    agentId,
    hasFolder,
    loading,
    error,
    data,
    size.width,
    size.height,
    onRequestPickFolder,
    onSelectFile,
  ]);

  const showRoot = Boolean(agentId && hasFolder && agentFolder);

  return (
    <div className="flex h-full w-full flex-col overflow-hidden p-2">
      {showRoot && agentFolder && <RootRow folderPath={agentFolder} />}
      <div
        ref={containerRef}
        className={cn("flex-1 overflow-hidden", showRoot && "pl-3")}
      >
        {content}
      </div>
    </div>
  );
}

/** Header row above the tree showing the agent's working directory.
 *  Same Folder icon as subfolders so it reads as the parent of what's below;
 *  the tree contents get a small left padding to make the parent/child
 *  relationship visible. Non-interactive for now. */
function RootRow({ folderPath }: { folderPath: string }) {
  const name = folderPath.split(/[/\\]/).filter(Boolean).pop() ?? folderPath;
  return (
    <div
      className="flex h-[26px] items-center gap-1.5 px-2 text-sm text-foreground"
      title={folderPath}
    >
      <FolderOpen className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1 truncate font-medium">{name}</span>
    </div>
  );
}

function Node({
  node,
  style,
  dragHandle,
}: NodeRendererProps<TreeNode>) {
  const isDir = node.data.isDir;
  return (
    <div
      ref={dragHandle}
      style={style}
      className={cn(
        "flex h-full items-center gap-1.5 rounded-sm px-2 text-sm",
        "cursor-pointer select-none hover:bg-accent/60",
        node.isSelected && "bg-accent",
      )}
      onClick={() => {
        if (isDir) node.toggle();
        else node.activate();
      }}
    >
      {isDir ? (
        node.isOpen ? (
          <FolderOpen className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        )
      ) : (
        (() => {
          const Icon = iconForFile(node.data.name);
          return <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />;
        })()
      )}
      <span
        className="min-w-0 flex-1 truncate"
        title={node.data.name}
      >
        {node.data.name}
      </span>
    </div>
  );
}

function entryToNode(e: FileEntry): TreeNode {
  return {
    id: e.path,
    name: e.name,
    path: e.path,
    isDir: e.isDir,
    children: e.isDir ? [] : undefined,
    loaded: false,
  };
}

/** Walk a TreeNode forest, applying `update` to the node whose `id` matches.
 *  Returns a new array (no in-place mutation) so React picks up the change. */
function mapTreeNode(
  nodes: TreeNode[],
  id: string,
  update: (n: TreeNode) => TreeNode,
): TreeNode[] {
  return nodes.map((n) => {
    if (n.id === id) return update(n);
    if (n.children && n.children.length > 0) {
      return { ...n, children: mapTreeNode(n.children, id, update) };
    }
    return n;
  });
}

function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description: string;
  action?: { label: string; onClick: () => void };
}) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 px-4 text-center">
      <div className="text-sm font-medium">{title}</div>
      <div className="text-xs text-muted-foreground">{description}</div>
      {action && (
        <button
          onClick={action.onClick}
          className="mt-1 rounded-md border border-border bg-surface px-3 py-1 text-xs shadow-subtle hover:bg-accent"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
