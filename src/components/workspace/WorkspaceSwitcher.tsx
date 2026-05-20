import { useEffect, useState } from "react";
import { ChevronsUpDown, Folders, Pencil, Plus, Trash2, Users } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { workspaceApi } from "@/lib/tauri";
import type { Workspace } from "@/types/auth";
import { WorkspaceMembersDialog } from "./WorkspaceMembersDialog";

type Props = {
  workspaces: Workspace[];
  activeWorkspace: Workspace | null;
  /** Admin (= Org-Owner) darf Workspaces anlegen, umbenennen, löschen
   *  und Mitglieder verwalten — siehe CLAUDE.md §4. */
  isAdmin: boolean;
  onSelect: (ws: Workspace) => void;
  /** Parent lädt die Workspace-Liste neu (nach Create/Rename/Delete). */
  onChanged: (selectId?: string) => void;
};

export function WorkspaceSwitcher({
  workspaces,
  activeWorkspace,
  isAdmin,
  onSelect,
  onChanged,
}: Props) {
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [membersOpen, setMembersOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [renameName, setRenameName] = useState("");

  // Sync the rename input each time the dialog opens for the active workspace.
  useEffect(() => {
    if (renaming && activeWorkspace) {
      setRenameName(activeWorkspace.name);
      setError(null);
    }
  }, [renaming, activeWorkspace]);

  async function create() {
    if (!newName.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const ws = await workspaceApi.create(newName.trim());
      setCreating(false);
      setNewName("");
      onChanged(ws.id);
    } catch (e) {
      setError(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(false);
    }
  }

  async function rename() {
    if (!activeWorkspace) return;
    const next = renameName.trim();
    if (!next || next === activeWorkspace.name) {
      setRenaming(false);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await workspaceApi.rename(activeWorkspace.id, next);
      setRenaming(false);
      onChanged(activeWorkspace.id);
    } catch (e) {
      setError(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!activeWorkspace) return;
    setBusy(true);
    try {
      await workspaceApi.delete(activeWorkspace.id);
      setConfirmDelete(false);
      onChanged();
    } catch (e) {
      setError(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex items-center gap-1 px-3 py-2">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="min-w-0 flex-1 justify-between gap-2 px-2 font-normal hover:bg-accent/60"
            title={activeWorkspace?.name}
          >
            <span className="flex min-w-0 items-center gap-2">
              <Folders className="h-4 w-4 shrink-0" />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">
                {activeWorkspace?.name ?? "Kein Workspace"}
              </span>
            </span>
            <ChevronsUpDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-60">
          <DropdownMenuLabel className="text-xs text-muted-foreground">
            Workspaces
          </DropdownMenuLabel>
          {workspaces.length === 0 && (
            <div className="px-2 py-1.5 text-xs text-muted-foreground">
              Noch keine Workspaces angelegt.
            </div>
          )}
          {workspaces.map((w) => (
            <DropdownMenuItem
              key={w.id}
              onSelect={() => onSelect(w)}
              className="gap-2"
            >
              <Folders className="h-4 w-4 shrink-0" />
              <span className="truncate">{w.name}</span>
            </DropdownMenuItem>
          ))}
          {isAdmin && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onSelect={() => {
                  setCreating(true);
                  setError(null);
                }}
                className="gap-2"
              >
                <Plus className="h-3.5 w-3.5" />
                Neuer Workspace
              </DropdownMenuItem>
              {activeWorkspace && (
                <DropdownMenuItem
                  onSelect={() => setRenaming(true)}
                  className="gap-2"
                >
                  <Pencil className="h-3.5 w-3.5" />
                  Workspace umbenennen
                </DropdownMenuItem>
              )}
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8 shrink-0"
        onClick={() => setMembersOpen(true)}
        disabled={!activeWorkspace}
        title="Mitglieder"
      >
        <Users className="h-3.5 w-3.5" />
      </Button>
      {isAdmin && (
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 shrink-0 hover:bg-destructive/15 hover:text-destructive"
          onClick={() => setConfirmDelete(true)}
          disabled={!activeWorkspace}
          title="Workspace löschen"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      )}

      <Dialog open={creating} onOpenChange={(v) => !v && setCreating(false)}>
        <DialogContent className="sm:max-w-[420px]">
          <DialogHeader>
            <DialogTitle>Neuer Workspace</DialogTitle>
          </DialogHeader>
          <Input
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            placeholder="Name"
            autoFocus
            onKeyDown={(e) => e.key === "Enter" && create()}
          />
          {error && (
            <div className="rounded-md border border-destructive/40 bg-destructive/15 px-3 py-2 text-xs text-destructive">
              {error}
            </div>
          )}
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setCreating(false)}
              disabled={busy}
            >
              Abbrechen
            </Button>
            <Button onClick={create} disabled={busy || !newName.trim()}>
              Anlegen
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={renaming} onOpenChange={(v) => !v && setRenaming(false)}>
        <DialogContent className="sm:max-w-[420px]">
          <DialogHeader>
            <DialogTitle>Workspace umbenennen</DialogTitle>
          </DialogHeader>
          <Input
            value={renameName}
            onChange={(e) => setRenameName(e.target.value)}
            placeholder="Neuer Name"
            autoFocus
            onKeyDown={(e) => e.key === "Enter" && rename()}
          />
          {error && (
            <div className="rounded-md border border-destructive/40 bg-destructive/15 px-3 py-2 text-xs text-destructive">
              {error}
            </div>
          )}
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setRenaming(false)}
              disabled={busy}
            >
              Abbrechen
            </Button>
            <Button
              onClick={rename}
              disabled={
                busy ||
                !renameName.trim() ||
                renameName.trim() === activeWorkspace?.name
              }
            >
              Speichern
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={confirmDelete}
        onOpenChange={(v) => !v && setConfirmDelete(false)}
      >
        <DialogContent className="sm:max-w-[440px]">
          <DialogHeader>
            <DialogTitle>Workspace löschen?</DialogTitle>
          </DialogHeader>
          <p className="text-xs text-muted-foreground">
            „{activeWorkspace?.name}" und alle zugehörigen Agenten, Dateien
            und Mitgliedschaften werden unwiderruflich entfernt.
          </p>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setConfirmDelete(false)}
              disabled={busy}
            >
              Abbrechen
            </Button>
            <Button variant="destructive" onClick={remove} disabled={busy}>
              Löschen
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {activeWorkspace && (
        <WorkspaceMembersDialog
          open={membersOpen}
          workspace={activeWorkspace}
          canManage={isAdmin}
          onClose={() => setMembersOpen(false)}
        />
      )}
    </div>
  );
}
