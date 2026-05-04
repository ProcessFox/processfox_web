import { useEffect, useState } from "react";

import { previewApi } from "@/lib/tauri";

type Props = {
  agentId: string;
  filePath: string;
};

type State =
  | { kind: "loading" }
  | { kind: "ready"; srcDoc: string }
  | { kind: "error"; message: string };

/** DOCX preview — renders a sanitized HTML body returned by the backend
 *  inside a sandboxed iframe. Sandbox keeps any future surprise (links,
 *  scripts, embeds) confined; we additionally avoid emitting `<script>`
 *  on the Rust side. */
export function DocxViewer({ agentId, filePath }: Props) {
  const [state, setState] = useState<State>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    setState({ kind: "loading" });
    previewApi
      .docx(agentId, filePath)
      .then((res) => {
        if (cancelled) return;
        setState({ kind: "ready", srcDoc: wrapInDocument(res.html) });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        const message =
          (e as { message?: string })?.message ?? String(e ?? "Unbekannter Fehler");
        setState({ kind: "error", message });
      });
    return () => {
      cancelled = true;
    };
  }, [agentId, filePath]);

  if (state.kind === "loading") {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-xs text-muted-foreground">
        Lädt …
      </div>
    );
  }
  if (state.kind === "error") {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-center text-xs text-destructive">
        {state.message}
      </div>
    );
  }
  return (
    <iframe
      srcDoc={state.srcDoc}
      title="DOCX-Vorschau"
      sandbox=""
      className="flex-1 border-0 bg-background"
    />
  );
}

/** Wrap the body fragment in a complete HTML document with print-style
 *  defaults — readable line length, generous spacing, table borders. The
 *  iframe has `sandbox=""` so styles can't leak in or out. */
function wrapInDocument(bodyHtml: string): string {
  return `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<style>
  :root { color-scheme: light dark; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, sans-serif;
    font-size: 14px;
    line-height: 1.6;
    color: #1b1c1d;
    background: #fdfcff;
    padding: 24px 32px;
    max-width: 78ch;
    margin: 0 auto;
  }
  @media (prefers-color-scheme: dark) {
    body { color: #e1e2e8; background: #1b1c1d; }
    table, td { border-color: #43474e; }
  }
  h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 1.4em 0 0.4em; }
  h1 { font-size: 1.6em; }
  h2 { font-size: 1.35em; }
  h3 { font-size: 1.15em; }
  p { margin: 0.4em 0; }
  ul, ol { padding-left: 1.5em; }
  li { margin: 0.15em 0; }
  table {
    border-collapse: collapse;
    margin: 0.8em 0;
    width: 100%;
  }
  td {
    border: 1px solid #c8cad0;
    padding: 4px 8px;
    vertical-align: top;
  }
  strong { font-weight: 600; }
</style>
</head>
<body>${bodyHtml}</body>
</html>`;
}
