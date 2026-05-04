import { X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { iconForFile } from "@/lib/fileIcons";

export type PreviewStatus =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "saved" }
  | { kind: "conflict" }
  | { kind: "error"; message: string };

type Props = {
  fileName: string;
  status?: PreviewStatus;
  onClose: () => void;
};

export function PreviewHeader({ fileName, status, onClose }: Props) {
  const Icon = iconForFile(fileName);
  return (
    <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border bg-surface px-3 py-2">
      <div className="flex min-w-0 items-center gap-2">
        <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate text-sm font-medium">{fileName}</span>
      </div>
      <div className="flex items-center gap-2">
        {status && <StatusLabel status={status} />}
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          onClick={onClose}
          title="Vorschau schließen"
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}

function StatusLabel({ status }: { status: PreviewStatus }) {
  if (status.kind === "idle") return null;
  if (status.kind === "saving") {
    return <span className="text-xs text-muted-foreground">Speichern …</span>;
  }
  if (status.kind === "saved") {
    return <span className="text-xs text-muted-foreground">Gespeichert</span>;
  }
  if (status.kind === "conflict") {
    return (
      <span className="text-xs text-amber-600 dark:text-amber-400">
        Externe Änderung
      </span>
    );
  }
  return (
    <span className="text-xs text-destructive" title={status.message}>
      Fehler
    </span>
  );
}
