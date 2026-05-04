import { ChevronsUpDown, Plus, Settings, Settings2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { DynamicIcon } from "@/components/ui/DynamicIcon";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { Agent } from "@/types/agent";

type Props = {
  agents: Agent[];
  activeAgent: Agent | null;
  onSelect: (agent: Agent) => void;
  onCreate: () => void;
  onEdit: () => void;
  onOpenSettings: () => void;
};

export function AgentSwitcher({
  agents,
  activeAgent,
  onSelect,
  onCreate,
  onEdit,
  onOpenSettings,
}: Props) {
  return (
    <div className="flex items-center gap-1 px-3 py-2">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="min-w-0 flex-1 justify-between gap-2 px-2 font-normal hover:bg-accent/60"
            title={activeAgent?.name}
          >
            <span className="flex min-w-0 items-center gap-2">
              <DynamicIcon
                name={activeAgent?.icon}
                className="h-4 w-4 shrink-0"
              />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">
                {activeAgent?.name ?? "Kein Agent"}
              </span>
            </span>
            <ChevronsUpDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-60">
          <DropdownMenuLabel className="text-xs text-muted-foreground">
            Agenten
          </DropdownMenuLabel>
          {agents.length === 0 && (
            <div className="px-2 py-1.5 text-xs text-muted-foreground">
              Noch keine Agenten angelegt.
            </div>
          )}
          {agents.map((a) => (
            <DropdownMenuItem
              key={a.id}
              onSelect={() => onSelect(a)}
              className="gap-2"
            >
              <DynamicIcon name={a.icon} className="h-4 w-4 shrink-0" />
              <span className="truncate">{a.name}</span>
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={onCreate} className="gap-2">
            <Plus className="h-3.5 w-3.5" />
            Neuer Agent
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8 shrink-0"
        onClick={onEdit}
        disabled={!activeAgent}
        title="Agent bearbeiten"
      >
        <Settings2 className="h-3.5 w-3.5" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8 shrink-0"
        onClick={onOpenSettings}
        title="Einstellungen"
      >
        <Settings className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}
