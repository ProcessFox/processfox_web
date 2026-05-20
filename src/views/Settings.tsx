import { useEffect, useState } from "react";

import { CloudApisTab } from "@/components/settings/CloudApisTab";
import { useTheme, type Theme } from "@/components/theme-provider";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { settingsApi } from "@/lib/tauri";
import type { Settings } from "@/types/settings";

type SettingsTab = "cloud" | "appearance" | "about";

type Props = {
  open: boolean;
  defaultTab?: SettingsTab;
  /** Nur Admin (Org-Owner) sieht den Cloud-APIs-Tab (CLAUDE.md §4). */
  isAdmin: boolean;
  onClose: () => void;
  onSettingsChange?: (s: Settings) => void;
  onLogout?: () => void;
};

const THEME_OPTIONS: { value: Theme; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Hell" },
  { value: "dark", label: "Dunkel" },
];

export function SettingsDialog({
  open,
  defaultTab = "cloud",
  isAdmin,
  onClose,
  onSettingsChange,
  onLogout,
}: Props) {
  const { theme, setTheme } = useTheme();
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    // Settings werden nur in der Cloud-APIs-Kachel angezeigt — sparen wir
    // uns für Nutzer, die den Tab gar nicht sehen.
    if (!open || !isAdmin) return;
    settingsApi.get().then(setSettings).catch(console.error);
  }, [open, isAdmin]);

  function handleSettingsChange(s: Settings) {
    setSettings(s);
    onSettingsChange?.(s);
  }

  // Admins behalten den vom Aufrufer gewünschten Default-Tab.
  // Nutzer ohne Cloud-Tab fallen sauber auf „appearance" zurück.
  const effectiveDefault: SettingsTab =
    !isAdmin && defaultTab === "cloud" ? "appearance" : defaultTab;

  return (
    <Dialog open={open} onOpenChange={(v) => !v && onClose()}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-[720px]">
        <DialogHeader>
          <DialogTitle>Einstellungen</DialogTitle>
        </DialogHeader>

        <Tabs defaultValue={effectiveDefault}>
          <TabsList className="w-full justify-start">
            {isAdmin && <TabsTrigger value="cloud">Cloud-APIs</TabsTrigger>}
            <TabsTrigger value="appearance">Darstellung</TabsTrigger>
            <TabsTrigger value="about">Über</TabsTrigger>
          </TabsList>

          {isAdmin && (
            <TabsContent value="cloud">
              <CloudApisTab
                settings={settings}
                onSettingsChange={handleSettingsChange}
              />
            </TabsContent>
          )}

          <TabsContent value="appearance" className="py-4">
            <div className="flex flex-col gap-3">
              <div className="text-sm font-medium">Theme</div>
              <div className="flex gap-2">
                {THEME_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    onClick={() => setTheme(opt.value)}
                    className={`rounded-md border px-3 py-1.5 text-xs transition-colors ${
                      theme === opt.value
                        ? "border-primary bg-primary/10 text-foreground"
                        : "border-border bg-background text-muted-foreground hover:bg-accent"
                    }`}
                  >
                    {opt.label}
                  </button>
                ))}
              </div>
            </div>
          </TabsContent>

          <TabsContent value="about" className="py-4">
            <div className="flex flex-col gap-3">
              <div className="flex flex-col gap-1 text-xs">
                <div className="text-sm font-medium">ProcessFox Web</div>
                <div className="text-muted-foreground">Version 0.1.0</div>
                <div className="text-muted-foreground">
                  Team-fähige KI-Agenten für gemeinsame Dokumentenarbeit.
                </div>
                {!isAdmin && (
                  <div className="mt-2 rounded-md border border-border bg-surface px-2 py-1.5 text-muted-foreground">
                    Du bist als Nutzer angemeldet. API-Keys, Default-Modell
                    und Workspace-Verwaltung sind Admins vorbehalten.
                  </div>
                )}
              </div>
              {onLogout && (
                <button
                  onClick={onLogout}
                  className="self-start rounded-md border border-border bg-background px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
                >
                  Abmelden
                </button>
              )}
            </div>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
