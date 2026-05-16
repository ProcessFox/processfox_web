import { useState } from "react";
import { Plus, Trash2, Users } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { workspaceApi } from "@/lib/tauri";
import type { Workspace } from "@/types/auth";
import { WorkspaceMembersDialog } from "./WorkspaceMembersDialog";

type Props = {
  workspaces: Workspace[];
  activeWorkspace: Workspace | null;
  isOwner: boolean;
  onSelect: (ws: Workspace) => void;
  /** Parent lädt die Workspace-Liste neu (nach Create/Delete). */
  onChanged: (selectId?: string) => void;
};

export function WorkspaceSwitcher({
  workspaces,
  activeWorkspace,
  isOwner,
  onSelect,
  onChanged,
}: Props) {
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [membersOpen, setMembersOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

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
    <div className="flex items-center gap-1.5 border-b border-border px-3 py-2">
      <select
        value={activeWorkspace?.id ?? ""}
        onChange={(e) => {
          const ws = workspaces.find((w) => w.id === e.target.value);
          if (ws) onSelect(ws);
        }}
        className="min-w-0 flex-1 rounded-md border border-border bg-background px-2 py-1 text-xs"
        disabled={workspaces.length === 0}
      >
        {workspaces.length === 0 && <option value="">Kein Workspace</option>}
        {workspaces.map((w) => (
          <option key={w.id} value={w.id}>
            {w.name}
          </option>
        ))}
      </select>

      {activeWorkspace && (
        <button
          onClick={() => setMembersOpen(true)}
          title="Mitglieder"
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent"
        >
          <Users className="h-3.5 w-3.5" />
        </button>
      )}
      {isOwner && (
        <button
          onClick={() => {
            setCreating(true);
            setError(null);
          }}
          title="Workspace anlegen"
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent"
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
      )}
      {isOwner && activeWorkspace && (
        <button
          onClick={() => setConfirmDelete(true)}
          title="Workspace löschen"
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
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
          canManage={isOwner}
          onClose={() => setMembersOpen(false)}
        />
      )}
    </div>
  );
}
