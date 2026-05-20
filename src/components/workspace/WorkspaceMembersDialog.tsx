import { useCallback, useEffect, useState } from "react";
import { Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { memberApi } from "@/lib/tauri";
import type { Workspace, WorkspaceMember } from "@/types/auth";

type Props = {
  open: boolean;
  workspace: Workspace;
  /** Admin (Org-Owner) darf Mitglieder einladen/entfernen (CLAUDE.md §4). */
  canManage: boolean;
  onClose: () => void;
};

export function WorkspaceMembersDialog({
  open,
  workspace,
  canManage,
  onClose,
}: Props) {
  const [members, setMembers] = useState<WorkspaceMember[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    memberApi
      .list(workspace.id)
      .then(setMembers)
      .catch((e) => setError(String((e as { message?: string })?.message ?? e)))
      .finally(() => setLoading(false));
  }, [workspace.id]);

  useEffect(() => {
    if (open) refresh();
  }, [open, refresh]);

  async function add() {
    if (!email.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await memberApi.add(workspace.id, email.trim());
      setEmail("");
      refresh();
    } catch (e) {
      setError(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(false);
    }
  }

  async function remove(userId: string) {
    try {
      await memberApi.remove(workspace.id, userId);
      refresh();
    } catch (e) {
      setError(String((e as { message?: string })?.message ?? e));
    }
  }

  return (
    <Dialog open={open} onOpenChange={(v) => !v && onClose()}>
      <DialogContent className="sm:max-w-[520px]">
        <DialogHeader>
          <DialogTitle>Mitglieder — {workspace.name}</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-4 py-2">
          {canManage ? (
            <div className="flex flex-col gap-2 rounded-md border border-border bg-surface p-3">
              <Label className="text-xs">Mitglied einladen</Label>
              <div className="flex gap-2">
                <Input
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="E-Mail (bereits registriertes Org-Mitglied)"
                  className="flex-1 text-xs"
                  onKeyDown={(e) => e.key === "Enter" && add()}
                />
                <Button size="sm" onClick={add} disabled={busy}>
                  Einladen
                </Button>
              </div>
            </div>
          ) : (
            <div className="rounded-md border border-border bg-surface px-3 py-2 text-xs text-muted-foreground">
              Nur Admins können Mitglieder einladen oder entfernen.
            </div>
          )}

          {error && (
            <div className="rounded-md border border-destructive/40 bg-destructive/15 px-3 py-2 text-xs text-destructive">
              {error}
            </div>
          )}

          <div className="flex flex-col gap-1">
            {loading ? (
              <div className="px-1 py-2 text-xs text-muted-foreground">
                Lädt …
              </div>
            ) : members.length === 0 ? (
              <div className="px-1 py-2 text-xs text-muted-foreground">
                Noch keine Mitglieder.
              </div>
            ) : (
              members.map((m) => (
                <div
                  key={m.userId}
                  className="flex items-center gap-2 rounded-sm px-1.5 py-1.5 text-sm hover:bg-accent/40"
                >
                  <span className="min-w-0 flex-1 truncate" title={m.email}>
                    {m.email}
                  </span>
                  {canManage && (
                    <button
                      onClick={() => remove(m.userId)}
                      className="flex h-7 w-7 items-center justify-center rounded-sm text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
                      title="Entfernen"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  )}
                </div>
              ))
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
