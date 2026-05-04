export interface CatalogEntry {
  id: string;
  title: string;
  vendor: string;
  quant: string;
  sizeBytes: number;
  minRamGb: number;
  hfUrl: string;
  filename: string;
  description: string;
}

export interface InstalledModel {
  filename: string;
  path: string;
  sizeBytes: number;
  catalogId: string | null;
}

export interface HardwareInfo {
  ramGb: number;
  recommendedModelId: string | null;
}

export type DownloadEvent =
  | { type: "started"; totalBytes: number | null }
  | { type: "progress"; received: number; total: number | null }
  | { type: "finished"; path: string; sizeBytes: number }
  | { type: "error"; message: string }
  | { type: "cancelled" };
