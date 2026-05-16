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
  fileId: string | null;
  fileName: string | null;
  onClose: () => void;
};

export function PreviewPane({ fileId, fileName, onClose }: Props) {
  // Each sub-viewer can publish its own status (saving/saved/conflict). The
  // header lifts it up so we don't have to re-render a header per viewer.
  const [status, setStatus] = useState<PreviewStatus>({ kind: "idle" });

  if (!fileName || !fileId) {
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
          fileId={fileId}
          fileName={fileName}
          onStatus={setStatus}
        />
      </div>
    </div>
  );
}

function Body({
  kind,
  fileId,
  fileName,
  onStatus,
}: {
  kind: ReturnType<typeof previewKindForName>;
  fileId: string;
  fileName: string;
  onStatus: (s: PreviewStatus) => void;
}) {
  switch (kind) {
    case "image":
      return <ImageViewer fileId={fileId} fileName={fileName} />;
    case "text":
      return <TextEditor fileId={fileId} onStatus={onStatus} />;
    case "markdown":
      return <MarkdownEditor fileId={fileId} onStatus={onStatus} />;
    case "pdf":
      return <PdfViewer fileId={fileId} />;
    case "docx":
      return <DocxViewer fileId={fileId} />;
    case "xlsx":
      return <XlsxViewer fileId={fileId} />;
    case "pptx":
      return <PptxViewer fileId={fileId} />;
    case "unsupported":
    default:
      return <UnsupportedViewer fileId={fileId} fileName={fileName} />;
  }
}
