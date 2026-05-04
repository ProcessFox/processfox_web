import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  Check,
  Cpu,
  Download,
  Sparkles,
  Square,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { modelsApi, settingsApi } from "@/lib/tauri";
import type {
  CatalogEntry,
  DownloadEvent,
  HardwareInfo,
  InstalledModel,
} from "@/types/models";
import type { Settings } from "@/types/settings";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / 1024 / 1024).toFixed(0)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

type DownloadState =
  | { status: "idle" }
  | { status: "starting" }
  | {
      status: "running";
      received: number;
      total: number | null;
    }
  | { status: "error"; message: string };

type Props = {
  settings: Settings | null;
  onSettingsChange: (s: Settings) => void;
};

export function ModelsTab({ settings, onSettingsChange }: Props) {
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [installed, setInstalled] = useState<InstalledModel[]>([]);
  const [pendingDelete, setPendingDelete] = useState<InstalledModel | null>(
    null,
  );
  const [deleting, setDeleting] = useState(false);
  const [hardware, setHardware] = useState<HardwareInfo | null>(null);
  const [downloads, setDownloads] = useState<Record<string, DownloadState>>({});
  const unlistenRefs = useRef<Record<string, UnlistenFn>>({});

  const refresh = useCallback(async () => {
    const [cat, inst] = await Promise.all([
      modelsApi.listCatalog(),
      modelsApi.listInstalled(),
    ]);
    setCatalog(cat);
    setInstalled(inst);
  }, []);

  useEffect(() => {
    refresh().catch(console.error);
    modelsApi.getHardwareInfo().then(setHardware).catch(console.error);

    return () => {
      // Clean up any active subscription handles on unmount.
      for (const u of Object.values(unlistenRefs.current)) u();
      unlistenRefs.current = {};
    };
  }, [refresh]);

  const installedByFilename = useMemo(() => {
    const map: Record<string, InstalledModel> = {};
    for (const m of installed) map[m.filename] = m;
    return map;
  }, [installed]);

  function setDownload(id: string, state: DownloadState) {
    setDownloads((prev) => ({ ...prev, [id]: state }));
  }

  async function subscribe(id: string) {
    // Avoid duplicate subscriptions.
    if (unlistenRefs.current[id]) return;
    const unlisten = await modelsApi.subscribeDownload(id, (event) => {
      handleDownloadEvent(id, event);
    });
    unlistenRefs.current[id] = unlisten;
  }

  function unsubscribe(id: string) {
    const u = unlistenRefs.current[id];
    if (u) {
      u();
      delete unlistenRefs.current[id];
    }
  }

  async function handleDownloadEvent(id: string, event: DownloadEvent) {
    switch (event.type) {
      case "started":
        setDownload(id, {
          status: "running",
          received: 0,
          total: event.totalBytes,
        });
        break;
      case "progress":
        setDownload(id, {
          status: "running",
          received: event.received,
          total: event.total,
        });
        break;
      case "finished": {
        unsubscribe(id);
        setDownload(id, { status: "idle" });
        await refresh();
        const parts = event.path.split(/[/\\]/);
        const filename = parts[parts.length - 1] ?? "";
        // If no default provider is set yet, wire this freshly downloaded
        // model up as the app-wide default so the chat is immediately usable.
        if (settings && !settings.defaultProvider && filename) {
          await settingsApi.setDefaultProvider("local");
          const withModel = await settingsApi.setDefaultModel("local", filename);
          onSettingsChange(withModel);
        }
        // Mark first-run as done on first successful download.
        if (settings && !settings.firstRunDone) {
          const next = await settingsApi.setFirstRunDone();
          onSettingsChange(next);
        }
        break;
      }
      case "cancelled":
        unsubscribe(id);
        setDownload(id, { status: "idle" });
        break;
      case "error":
        unsubscribe(id);
        setDownload(id, { status: "error", message: event.message });
        break;
    }
  }

  async function startCatalogDownload(entry: CatalogEntry) {
    setDownload(entry.id, { status: "starting" });
    try {
      await subscribe(entry.id);
      await modelsApi.downloadFromCatalog(entry.id);
    } catch (e) {
      unsubscribe(entry.id);
      setDownload(entry.id, {
        status: "error",
        message: String((e as { message?: string })?.message ?? e),
      });
    }
  }

  async function cancelDownload(id: string) {
    try {
      await modelsApi.cancelDownload(id);
    } catch (e) {
      console.warn("cancel failed", e);
    }
  }

  function requestDelete(filename: string) {
    const entry = installed.find((m) => m.filename === filename);
    if (entry) setPendingDelete(entry);
  }

  async function confirmDelete() {
    if (!pendingDelete) return;
    setDeleting(true);
    try {
      await modelsApi.deleteModel(pendingDelete.filename);
      await refresh();
      setPendingDelete(null);
    } catch (e) {
      console.error(e);
    } finally {
      setDeleting(false);
    }
  }

  return (
    <div className="flex flex-col gap-4 py-2">
      <HardwareBanner hardware={hardware} catalog={catalog} />

      <div className="flex flex-col gap-3">
        {catalog.map((entry) => {
          const isInstalled = Boolean(installedByFilename[entry.filename]);
          const dl = downloads[entry.id] ?? { status: "idle" };
          const isRecommended =
            hardware?.recommendedModelId === entry.id && !isInstalled;
          return (
            <CatalogCard
              key={entry.id}
              entry={entry}
              isInstalled={isInstalled}
              isRecommended={isRecommended}
              download={dl}
              onDownload={() => startCatalogDownload(entry)}
              onCancel={() => cancelDownload(entry.id)}
              onDelete={() => requestDelete(entry.filename)}
              onDismissError={() =>
                setDownload(entry.id, { status: "idle" })
              }
            />
          );
        })}
      </div>

      <CustomUrlDownload
        installedByFilename={installedByFilename}
        downloads={downloads}
        onStart={async (url, filename, downloadId) => {
          setDownload(downloadId, { status: "starting" });
          try {
            await subscribe(downloadId);
            await modelsApi.downloadFromUrl(downloadId, url, filename);
          } catch (e) {
            unsubscribe(downloadId);
            setDownload(downloadId, {
              status: "error",
              message: String((e as { message?: string })?.message ?? e),
            });
          }
        }}
        onCancel={cancelDownload}
      />

      <Dialog
        open={pendingDelete !== null}
        onOpenChange={(v) => !v && !deleting && setPendingDelete(null)}
      >
        <DialogContent className="sm:max-w-[440px]">
          <DialogHeader>
            <DialogTitle>Modell löschen?</DialogTitle>
            <DialogDescription className="pt-2 text-foreground">
              <span className="font-mono text-xs">
                {pendingDelete?.filename}
              </span>
              <span className="mt-2 block text-muted-foreground">
                {pendingDelete
                  ? `Diese Datei (${formatBytes(pendingDelete.sizeBytes)}) wird sofort von der Festplatte entfernt — kein Papierkorb. Erneuter Download ist jederzeit aus dem Katalog möglich.`
                  : ""}
              </span>
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setPendingDelete(null)}
              disabled={deleting}
            >
              Abbrechen
            </Button>
            <Button
              variant="destructive"
              onClick={confirmDelete}
              disabled={deleting}
            >
              {deleting ? "Lösche …" : "Löschen"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function HardwareBanner({
  hardware,
  catalog,
}: {
  hardware: HardwareInfo | null;
  catalog: CatalogEntry[];
}) {
  if (!hardware) {
    return (
      <div className="rounded-md border border-border bg-surface p-3 text-xs text-muted-foreground">
        Ermittle Hardware …
      </div>
    );
  }
  const recommended = hardware.recommendedModelId
    ? catalog.find((m) => m.id === hardware.recommendedModelId)
    : null;
  return (
    <div className="flex items-start gap-3 rounded-md border border-border bg-surface p-3">
      <Cpu className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
      <div className="flex-1 text-xs">
        <div className="text-sm font-medium">
          {hardware.ramGb} GB Arbeitsspeicher erkannt
        </div>
        {recommended ? (
          <div className="mt-0.5 text-muted-foreground">
            Empfehlung für deine Hardware:{" "}
            <span className="font-medium text-foreground">
              {recommended.title}
            </span>{" "}
            ({formatBytes(recommended.sizeBytes)}, {recommended.quant}).
          </div>
        ) : (
          <div className="mt-0.5 text-muted-foreground">
            Keines der kuratierten Modelle passt komfortabel — du kannst
            trotzdem ein kleineres laden oder eine Cloud-API nutzen.
          </div>
        )}
      </div>
    </div>
  );
}

function CatalogCard({
  entry,
  isInstalled,
  isRecommended,
  download,
  onDownload,
  onCancel,
  onDelete,
  onDismissError,
}: {
  entry: CatalogEntry;
  isInstalled: boolean;
  isRecommended: boolean;
  download: DownloadState;
  onDownload: () => void;
  onCancel: () => void;
  onDelete: () => void;
  onDismissError: () => void;
}) {
  return (
    <div className="rounded-md border border-border bg-surface p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <div className="text-sm font-medium">{entry.title}</div>
            {isRecommended && (
              <span className="flex items-center gap-1 rounded-sm border border-primary/40 bg-primary/10 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-primary">
                <Sparkles className="h-2.5 w-2.5" />
                Empfohlen
              </span>
            )}
            {isInstalled && (
              <span className="flex items-center gap-1 rounded-sm border border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-emerald-600 dark:text-emerald-400">
                <Check className="h-2.5 w-2.5" />
                Installiert
              </span>
            )}
          </div>
          <div className="mt-0.5 text-xs text-muted-foreground">
            {entry.vendor} · {entry.quant} · {formatBytes(entry.sizeBytes)} ·
            ab {entry.minRamGb} GB RAM
          </div>
          <div className="mt-1.5 text-xs text-muted-foreground">
            {entry.description}
          </div>
        </div>

        <div className="shrink-0">
          {isInstalled ? (
            <Button
              variant="ghost"
              size="icon"
              onClick={onDelete}
              title="Modell entfernen"
              className="h-8 w-8"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          ) : download.status === "running" ||
            download.status === "starting" ? (
            <Button
              variant="outline"
              size="sm"
              onClick={onCancel}
              className="gap-1.5"
            >
              <Square className="h-3 w-3" />
              Stopp
            </Button>
          ) : (
            <Button size="sm" onClick={onDownload} className="gap-1.5">
              <Download className="h-3.5 w-3.5" />
              Download
            </Button>
          )}
        </div>
      </div>

      {(download.status === "running" || download.status === "starting") && (
        <div className="mt-3">
          <ProgressBar
            received={
              download.status === "running" ? download.received : 0
            }
            total={
              download.status === "running" ? download.total : entry.sizeBytes
            }
          />
        </div>
      )}

      {download.status === "error" && (
        <div className="mt-3 flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/15 px-3 py-2 text-xs text-destructive">
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <div className="flex-1">{download.message}</div>
          <button
            onClick={onDismissError}
            className="text-destructive/70 hover:text-destructive"
          >
            Schließen
          </button>
        </div>
      )}
    </div>
  );
}

function ProgressBar({
  received,
  total,
}: {
  received: number;
  total: number | null;
}) {
  const percent = total && total > 0 ? (received / total) * 100 : null;
  return (
    <div className="flex flex-col gap-1">
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
        {percent !== null ? (
          <div
            className="h-full bg-primary transition-all"
            style={{ width: `${Math.min(100, percent).toFixed(1)}%` }}
          />
        ) : (
          <div className="h-full w-1/3 animate-pulse bg-primary/60" />
        )}
      </div>
      <div className="flex justify-between text-xs text-muted-foreground">
        <span>
          {formatBytes(received)}
          {total ? ` / ${formatBytes(total)}` : ""}
        </span>
        {percent !== null && <span>{percent.toFixed(1)} %</span>}
      </div>
    </div>
  );
}

function CustomUrlDownload({
  installedByFilename,
  downloads,
  onStart,
  onCancel,
}: {
  installedByFilename: Record<string, InstalledModel>;
  downloads: Record<string, DownloadState>;
  onStart: (url: string, filename: string, downloadId: string) => Promise<void>;
  onCancel: (downloadId: string) => Promise<void>;
}) {
  const [url, setUrl] = useState("");
  const [downloadId, setDownloadId] = useState<string | null>(null);

  const filename = useMemo(() => deriveFilename(url), [url]);
  const alreadyInstalled = filename
    ? Boolean(installedByFilename[filename])
    : false;
  const currentState = downloadId ? downloads[downloadId] : undefined;
  const inflight =
    currentState?.status === "running" || currentState?.status === "starting";

  async function start() {
    if (!filename || alreadyInstalled) return;
    const id = `custom-${crypto.randomUUID()}`;
    setDownloadId(id);
    await onStart(url.trim(), filename, id);
  }

  return (
    <div className="rounded-md border border-dashed border-border bg-muted/30 p-4">
      <div className="text-sm font-medium">Eigene GGUF-URL</div>
      <div className="mt-0.5 text-xs text-muted-foreground">
        Direkter Link auf eine <code>.gguf</code>-Datei (z. B. von
        HuggingFace).
      </div>
      <div className="mt-3 flex flex-col gap-2">
        <Label className="text-xs">URL</Label>
        <div className="flex gap-2">
          <Input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://huggingface.co/…/modell.gguf"
            className="text-xs"
            disabled={inflight}
          />
          {inflight ? (
            <Button
              variant="outline"
              size="sm"
              onClick={() => downloadId && onCancel(downloadId)}
              className="gap-1.5"
            >
              <Square className="h-3 w-3" />
              Stopp
            </Button>
          ) : (
            <Button
              size="sm"
              onClick={start}
              disabled={!filename || alreadyInstalled}
              className="gap-1.5"
            >
              <Download className="h-3.5 w-3.5" />
              Download
            </Button>
          )}
        </div>
        {filename && (
          <div className="text-xs text-muted-foreground">
            Speichert als{" "}
            <span className="text-foreground">{filename}</span>
            {alreadyInstalled && " — bereits installiert."}
          </div>
        )}
        {currentState?.status === "running" && (
          <ProgressBar
            received={currentState.received}
            total={currentState.total}
          />
        )}
        {currentState?.status === "error" && (
          <div className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/15 px-3 py-2 text-xs text-destructive">
            <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <div className="flex-1">{currentState.message}</div>
          </div>
        )}
      </div>
    </div>
  );
}

function deriveFilename(url: string): string | null {
  const trimmed = url.trim();
  if (trimmed.length === 0) return null;
  try {
    const parsed = new URL(trimmed);
    const last = parsed.pathname.split("/").filter(Boolean).pop();
    if (!last) return null;
    if (!last.toLowerCase().endsWith(".gguf")) return null;
    // Basic sanity: no traversal, no colon.
    if (last.includes("..") || last.includes(":")) return null;
    return last;
  } catch {
    return null;
  }
}
