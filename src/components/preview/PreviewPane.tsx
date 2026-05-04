import { useState } from "react";

import { DocxViewer } from "./DocxViewer";
import { ImageViewer } from "./ImageViewer";
import { MarkdownEditor } from "./MarkdownEditor";
import { PdfViewer } from "./PdfViewer";
import { PptxViewer } from "./PptxViewer";
import { PreviewHeader, type PreviewStatus } from "./PreviewHeader";
import { TextEditor } from "./TextEditor";
import { UnsupportedViewer } from "./UnsupportedViewer";
import { XlsxViewer } from "./XlsxViewer";
import { previewKindForName } from "./previewKind";

type Props = {
  agentId: string | null;
  fileName: string | null;
  filePath: string | null;
  onClose: () => void;
};

export function PreviewPane({ agentId, fileName, filePath, onClose }: Props) {
  // Each sub-viewer can publish its own status (saving/saved/conflict). The
  // header lifts it up so we don't have to re-render a header per viewer.
  const [status, setStatus] = useState<PreviewStatus>({ kind: "idle" });

  if (!fileName || !filePath || !agentId) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-xs text-muted-foreground">
        Wähle links eine Datei, um ihre Vorschau zu öffnen.
      </div>
    );
  }

  const kind = previewKindForName(fileName);

  return (
    <div className="flex h-full flex-col">
      <PreviewHeader
        fileName={fileName}
        status={status}
        onClose={onClose}
      />
      <div className="flex min-h-0 flex-1 flex-col">
        <Body
          kind={kind}
          agentId={agentId}
          filePath={filePath}
          fileName={fileName}
          onStatus={setStatus}
        />
      </div>
    </div>
  );
}

function Body({
  kind,
  agentId,
  filePath,
  fileName,
  onStatus,
}: {
  kind: ReturnType<typeof previewKindForName>;
  agentId: string;
  filePath: string;
  fileName: string;
  onStatus: (s: PreviewStatus) => void;
}) {
  switch (kind) {
    case "image":
      return <ImageViewer filePath={filePath} fileName={fileName} />;
    case "text":
      return (
        <TextEditor
          agentId={agentId}
          filePath={filePath}
          onStatus={onStatus}
        />
      );
    case "markdown":
      return (
        <MarkdownEditor
          agentId={agentId}
          filePath={filePath}
          onStatus={onStatus}
        />
      );
    case "pdf":
      return <PdfViewer filePath={filePath} />;
    case "docx":
      return <DocxViewer agentId={agentId} filePath={filePath} />;
    case "xlsx":
      return <XlsxViewer agentId={agentId} filePath={filePath} />;
    case "pptx":
      return <PptxViewer agentId={agentId} filePath={filePath} />;
    case "unsupported":
    default:
      return <UnsupportedViewer filePath={filePath} fileName={fileName} />;
  }
}
