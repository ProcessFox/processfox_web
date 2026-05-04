import {
  Check,
  ChevronRight,
  FileEdit,
  FilePen,
  FilePlus,
  FileStack,
  FileText,
  Loader2,
  Sheet as SheetIcon,
  Workflow,
  X,
} from "lucide-react";
import type { ReactNode } from "react";
import { diffLines } from "diff";

import { Button } from "@/components/ui/button";
import type { HitlPreview, PendingHitl } from "@/types/chat";

type Props = {
  hitl: PendingHitl;
  busy?: boolean;
  onApprove: () => void;
  onReject: () => void;
};

export function HitlCard({ hitl, busy, onApprove, onReject }: Props) {
  const { preview } = hitl;
  const heading = headingFor(preview);
  return (
    <div className="flex flex-col gap-2.5 rounded-md border border-amber-500/40 bg-amber-500/15 p-3 text-xs text-amber-800 dark:text-amber-200">
      <div className="flex items-center gap-2">
        <heading.Icon className="h-3.5 w-3.5 shrink-0" />
        <span className="text-sm font-medium">{heading.label}</span>
        <span
          className="ml-auto truncate font-mono text-xs opacity-60"
          title={`Tool: ${hitl.toolName}`}
        >
          {hitl.toolName}
        </span>
      </div>

      <PathSummary preview={preview} />

      <PreviewBody preview={preview} />

      <div className="flex justify-end gap-2 pt-1">
        <Button
          size="sm"
          variant="ghost"
          onClick={onReject}
          disabled={busy}
          className="gap-1.5"
        >
          <X className="h-3.5 w-3.5" />
          Ablehnen
        </Button>
        <Button size="sm" onClick={onApprove} disabled={busy} className="gap-1.5">
          {busy ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Check className="h-3.5 w-3.5" />
          )}
          Freigeben
        </Button>
      </div>
    </div>
  );
}

function SectionLabel({ children }: { children: ReactNode }) {
  return <div className="text-xs font-medium opacity-90">{children}</div>;
}

function PathRow({ label, path }: { label: string; path: string }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="shrink-0 text-xs font-medium opacity-80">{label}</span>
      <span
        className="min-w-0 truncate font-mono text-xs opacity-90"
        title={path}
      >
        {path}
      </span>
    </div>
  );
}

function PathSummary({ preview }: { preview: HitlPreview }) {
  if (preview.kind === "writeDocxFromTemplate") {
    return (
      <div className="flex flex-col gap-1">
        <PathRow label="Vorlage" path={preview.templatePath} />
        <PathRow label="Ausgabe" path={preview.outputPath} />
      </div>
    );
  }
  return <PathRow label="Datei" path={preview.path} />;
}

function headingFor(preview: HitlPreview): {
  Icon: typeof FilePlus;
  label: string;
} {
  switch (preview.kind) {
    case "appendToFile":
      return preview.createsFile
        ? { Icon: FilePlus, label: "Neue Datei erstellen" }
        : { Icon: FilePen, label: "Inhalt anhängen" };
    case "writeDocx":
      return preview.createsFile
        ? { Icon: FilePlus, label: "Word-Dokument erstellen" }
        : { Icon: FileText, label: "Word-Dokument überschreiben" };
    case "appendToDocx":
      return preview.createsFile
        ? { Icon: FilePlus, label: "Word-Dokument erstellen" }
        : { Icon: FilePen, label: "Word-Dokument erweitern" };
    case "rewriteFile":
      return preview.createsFile
        ? { Icon: FilePlus, label: "Neue Datei schreiben" }
        : { Icon: FileEdit, label: "Datei komplett ersetzen" };
    case "updateCells": {
      const n = preview.changes.length;
      return {
        Icon: SheetIcon,
        label: `${n} ${n === 1 ? "Zelle" : "Zellen"} aktualisieren`,
      };
    }
    case "writeXlsx":
      return preview.createsFile
        ? { Icon: FilePlus, label: "Excel-Tabelle erstellen" }
        : { Icon: SheetIcon, label: "Excel-Tabelle überschreiben" };
    case "writeDocxFromTemplate":
      return preview.createsFile
        ? { Icon: FileStack, label: "Word-Dokument aus Vorlage erstellen" }
        : { Icon: FileStack, label: "Word-Dokument aus Vorlage überschreiben" };
    case "delegateIntoXlsxColumn": {
      const n = preview.rowCount;
      return {
        Icon: Workflow,
        label: `${n} ${n === 1 ? "Zeile" : "Zeilen"} per Worker bearbeiten`,
      };
    }
  }
}

function PreviewBody({ preview }: { preview: HitlPreview }) {
  switch (preview.kind) {
    case "appendToFile":
      return (
        <>
          {preview.existingTail && (
            <Section
              label="Bisheriger Inhalt (Ende)"
              subdued
              defaultOpen={false}
            >
              {preview.existingTail}
            </Section>
          )}
          <Section
            label={preview.existingTail ? "Anzuhängender Inhalt" : "Inhalt"}
          >
            {preview.content}
          </Section>
        </>
      );
    case "writeDocx":
      return (
        <>
          <Section
            label={`Inhalt — ${preview.blockCount} ${
              preview.blockCount === 1 ? "Block" : "Blöcke"
            }`}
          >
            {preview.previewText}
          </Section>
          {!preview.createsFile && (
            <p className="text-xs opacity-80">
              ⚠ Diese Datei existiert bereits und wird komplett überschrieben.
              Vorhandene Formatierung geht verloren — für Erweiterung benutze
              den Skill „Dokument fortschreiben".
            </p>
          )}
        </>
      );
    case "appendToDocx":
      return (
        <>
          {preview.existingTail && (
            <Section
              label="Bisheriger Inhalt (Ende, als Text)"
              subdued
              defaultOpen={false}
            >
              {preview.existingTail}
            </Section>
          )}
          <Section
            label={`Anzuhängender Inhalt — ${preview.blockCount} ${
              preview.blockCount === 1 ? "Block" : "Blöcke"
            }`}
          >
            {preview.previewText}
          </Section>
        </>
      );
    case "rewriteFile":
      return <DiffSection before={preview.before} after={preview.after} />;
    case "updateCells":
      return (
        <CellChangesSection
          sheet={preview.sheet}
          changes={preview.changes}
        />
      );
    case "writeXlsx":
      return (
        <WriteXlsxSection
          sheet={preview.sheet}
          rows={preview.rows}
          createsFile={preview.createsFile}
        />
      );
    case "writeDocxFromTemplate":
      return (
        <TemplateSection
          replacements={preview.replacements}
          templatePlaceholders={preview.templatePlaceholders}
          createsFile={preview.createsFile}
        />
      );
    case "delegateIntoXlsxColumn":
      return (
        <DelegateXlsxSection
          sheet={preview.sheet}
          targetColumn={preview.targetColumn}
          targetCreatesColumn={preview.targetCreatesColumn}
          rowCount={preview.rowCount}
          workerLabel={preview.workerLabel}
          samplePrompts={preview.samplePrompts}
        />
      );
  }
}

function DelegateXlsxSection({
  sheet,
  targetColumn,
  targetCreatesColumn,
  rowCount,
  workerLabel,
  samplePrompts,
}: {
  sheet: string;
  targetColumn: string;
  targetCreatesColumn: boolean;
  rowCount: number;
  workerLabel: string;
  samplePrompts: { rowLabel: string; renderedPrompt: string }[];
}) {
  return (
    <>
      <div className="flex flex-col gap-1">
        <SectionLabel>
          Sheet „{sheet}" — Spalte „{targetColumn}"
          {targetCreatesColumn ? " (wird neu angelegt)" : " (wird überschrieben)"}
        </SectionLabel>
        <div className="text-xs opacity-90">
          {rowCount} {rowCount === 1 ? "Zeile" : "Zeilen"} werden seriell vom
          Worker bearbeitet.
        </div>
        <div className="text-xs opacity-80">
          Worker:{" "}
          <span className="font-mono opacity-90">{workerLabel}</span>
        </div>
      </div>
      {samplePrompts.length > 0 && (
        <Section
          label={`Beispiel-Prompts (${samplePrompts.length} von ${rowCount})`}
          defaultOpen
        >
          {samplePrompts
            .map((s) => `── ${s.rowLabel} ──\n${s.renderedPrompt}`)
            .join("\n\n")}
        </Section>
      )}
      {!targetCreatesColumn && (
        <p className="text-xs opacity-80">
          ⚠ Die Spalte „{targetColumn}" enthält ggf. bereits Werte. Diese werden
          beim Lauf überschrieben.
        </p>
      )}
    </>
  );
}

function Section({
  label,
  children,
  subdued,
  defaultOpen = true,
}: {
  label: string;
  children: string;
  subdued?: boolean;
  defaultOpen?: boolean;
}) {
  return (
    <details open={defaultOpen} className="group">
      <summary className="flex cursor-pointer list-none items-center gap-1 text-xs font-medium opacity-90 hover:opacity-100 [&::-webkit-details-marker]:hidden">
        <ChevronRight className="h-3 w-3 shrink-0 transition-transform group-open:rotate-90" />
        {label}
      </summary>
      <pre
        className={`mt-1 max-h-48 overflow-auto rounded-sm border border-amber-500/30 ${
          subdued ? "bg-background/40 opacity-80" : "bg-background/60"
        } p-2 font-mono text-xs whitespace-pre-wrap`}
      >
        {children}
      </pre>
    </details>
  );
}

function WriteXlsxSection({
  sheet,
  rows,
  createsFile,
}: {
  sheet: string;
  rows: string[][];
  createsFile: boolean;
}) {
  const MAX_ROWS = 10;
  const visible = rows.slice(0, MAX_ROWS);
  const hidden = rows.length - visible.length;
  const colCount = rows.reduce((m, r) => Math.max(m, r.length), 0);
  return (
    <>
      <div className="flex flex-col gap-1">
        <SectionLabel>
          Sheet „{sheet}" — {rows.length}{" "}
          {rows.length === 1 ? "Zeile" : "Zeilen"} × {colCount}{" "}
          {colCount === 1 ? "Spalte" : "Spalten"}
        </SectionLabel>
        <div className="overflow-auto rounded-sm border border-amber-500/30 bg-background/60 font-mono text-xs">
          <table className="w-full">
            <tbody>
              {visible.map((row, i) => (
                <tr
                  key={i}
                  className={`border-b border-amber-500/20 last:border-b-0 ${
                    i === 0 ? "font-medium" : ""
                  }`}
                >
                  {Array.from({ length: colCount }).map((_, j) => (
                    <td key={j} className="border-r border-amber-500/10 px-2 py-0.5 last:border-r-0">
                      {row[j] ?? ""}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {hidden > 0 && (
          <div className="text-xs opacity-70">
            … +{hidden} weitere {hidden === 1 ? "Zeile" : "Zeilen"} (nicht angezeigt)
          </div>
        )}
      </div>
      {!createsFile && (
        <p className="text-xs opacity-80">
          ⚠ Diese Datei existiert bereits und wird komplett ersetzt — vorhandene
          Sheets, Formatierung und Formeln gehen verloren. Für gezielte Änderungen
          benutze den Skill „Tabelle ändern".
        </p>
      )}
    </>
  );
}

function TemplateSection({
  replacements,
  templatePlaceholders,
  createsFile,
}: {
  replacements: { key: string; value: string }[];
  templatePlaceholders: string[];
  createsFile: boolean;
}) {
  const providedKeys = new Set(replacements.map((r) => r.key));
  const missing = templatePlaceholders.filter((k) => !providedKeys.has(k));
  return (
    <>
      <div className="flex flex-col gap-1">
        <SectionLabel>
          Platzhalter füllen — {replacements.length}{" "}
          {replacements.length === 1 ? "Eintrag" : "Einträge"}
        </SectionLabel>
        <div className="overflow-auto rounded-sm border border-amber-500/30 bg-background/60 text-xs">
          <table className="w-full">
            <thead>
              <tr className="border-b border-amber-500/30 text-left opacity-70">
                <th className="px-2 py-1 font-medium">Schlüssel</th>
                <th className="px-2 py-1 font-medium">Wert</th>
              </tr>
            </thead>
            <tbody>
              {replacements.map((r) => (
                <tr
                  key={r.key}
                  className="border-b border-amber-500/20 last:border-b-0"
                >
                  <td className="px-2 py-1 align-top font-mono">{`{{${r.key}}}`}</td>
                  <td className="px-2 py-1 align-top whitespace-pre-wrap">
                    {r.value || <span className="opacity-50">(leer)</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      {missing.length > 0 && (
        <p className="text-xs opacity-80">
          ⚠ Die Vorlage enthält noch ungefüllte Platzhalter:{" "}
          {missing.map((k) => `{{${k}}}`).join(", ")}. Wenn diese im
          Ergebnis-Dokument als Klartext erscheinen sollen, ist das ok — sonst
          ablehnen und die Werte ergänzen.
        </p>
      )}
      {!createsFile && (
        <p className="text-xs opacity-80">
          ⚠ Die Ausgabedatei existiert bereits und wird ersetzt.
        </p>
      )}
    </>
  );
}

function CellChangesSection({
  sheet,
  changes,
}: {
  sheet: string;
  changes: { cell: string; before: string; after: string }[];
}) {
  return (
    <div className="flex flex-col gap-1">
      <SectionLabel>Sheet „{sheet}" — Änderungen</SectionLabel>
      <div className="overflow-auto rounded-sm border border-amber-500/30 bg-background/60 text-xs">
        <table className="w-full">
          <thead>
            <tr className="border-b border-amber-500/30 text-left opacity-70">
              <th className="px-2 py-1 font-medium">Zelle</th>
              <th className="px-2 py-1 font-medium">Vorher</th>
              <th className="px-2 py-1 font-medium">Nachher</th>
            </tr>
          </thead>
          <tbody>
            {changes.map((c, i) => (
              <tr
                key={`${c.cell}-${i}`}
                className="border-b border-amber-500/20 last:border-b-0"
              >
                <td className="px-2 py-1 font-mono font-medium">{c.cell}</td>
                <td className="px-2 py-1 align-top font-mono text-rose-700 line-through opacity-80 dark:text-rose-300">
                  {c.before || <span className="opacity-50">(leer)</span>}
                </td>
                <td className="px-2 py-1 align-top font-mono text-emerald-700 dark:text-emerald-300">
                  {c.after || <span className="opacity-50">(leer)</span>}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function DiffSection({ before, after }: { before: string; after: string }) {
  const parts = diffLines(before, after);
  return (
    <details open className="group">
      <summary className="flex cursor-pointer list-none items-center gap-1 text-xs font-medium opacity-90 hover:opacity-100 [&::-webkit-details-marker]:hidden">
        <ChevronRight className="h-3 w-3 shrink-0 transition-transform group-open:rotate-90" />
        Änderungen
      </summary>
      <div className="mt-1 max-h-64 overflow-auto rounded-sm border border-amber-500/30 bg-background/60 font-mono text-xs">
        {parts.length === 0 ? (
          <div className="px-2 py-1 italic opacity-70">Keine Änderungen.</div>
        ) : (
          parts.map((p, i) => {
            const cls = p.added
              ? "bg-emerald-500/15 text-emerald-800 dark:text-emerald-200"
              : p.removed
                ? "bg-rose-500/15 text-rose-800 dark:text-rose-200"
                : "opacity-70";
            const prefix = p.added ? "+ " : p.removed ? "- " : "  ";
            const lines = p.value.replace(/\n$/, "").split("\n");
            return (
              <div key={i} className={cls}>
                {lines.map((line, j) => (
                  <div key={j} className="whitespace-pre-wrap px-2">
                    {prefix}
                    {line}
                  </div>
                ))}
              </div>
            );
          })
        )}
      </div>
    </details>
  );
}
