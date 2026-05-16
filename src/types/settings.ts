/**
 * Settings sind pro Organisation (CLAUDE.md §10): ein Default-Provider
 * + Default-Modell-String. Kein lokales Modell, keine Hardware.
 */
export interface Settings {
  defaultProvider: string | null;
  defaultModel: string | null;
  /** Erstes Setup für die Org abgeschlossen (steuert den Welcome-Dialog). */
  firstRunDone: boolean;
}
