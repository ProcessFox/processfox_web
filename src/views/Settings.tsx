import { useEffect, useState } from "react";

import { CloudApisTab } from "@/components/settings/CloudApisTab";
import { ModelsTab } from "@/components/settings/ModelsTab";
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

type Props = {
  open: boolean;
  defaultTab?: "models" | "cloud" | "appearance" | "about";
  onClose: () => void;
  onSettingsChange?: (s: Settings) => void;
};

const THEME_OPTIONS: { value: Theme; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Hell" },
  { value: "dark", label: "Dunkel" },
];

export function SettingsDialog({
  open,
  defaultTab = "cloud",
  onClose,
  onSettingsChange,
}: Props) {
  const { theme, setTheme } = useTheme();
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    if (!open) return;
    settingsApi.get().then(setSettings).catch(console.error);
  }, [open]);

  function handleSettingsChange(s: Settings) {
    setSettings(s);
    onSettingsChange?.(s);
  }

  return (
    <Dialog open={open} onOpenChange={(v) => !v && onClose()}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-[720px]">
        <DialogHeader>
          <DialogTitle>Einstellungen</DialogTitle>
        </DialogHeader>

        <Tabs defaultValue={defaultTab}>
          <TabsList className="w-full justify-start">
            <TabsTrigger value="models">Modelle</TabsTrigger>
            <TabsTrigger value="cloud">Cloud-APIs</TabsTrigger>
            <TabsTrigger value="appearance">Darstellung</TabsTrigger>
            <TabsTrigger value="about">Über</TabsTrigger>
          </TabsList>

          <TabsContent value="models">
            <ModelsTab
              settings={settings}
              onSettingsChange={handleSettingsChange}
            />
          </TabsContent>

          <TabsContent value="cloud">
            <CloudApisTab
              settings={settings}
              onSettingsChange={handleSettingsChange}
            />
          </TabsContent>

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
            <div className="flex flex-col gap-1 text-xs">
              <div className="text-sm font-medium">ProcessFox</div>
              <div className="text-muted-foreground">
                Version 0.1.0 (Phase 2 — Etappe A)
              </div>
              <div className="text-muted-foreground">
                Lokale KI-Agenten für Einsteiger.
              </div>
            </div>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
