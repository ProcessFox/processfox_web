import { useEffect, useState } from "react";

import { fileApi } from "@/lib/tauri";

type Props = {
  fileId: string;
  fileName: string;
};

/** Image preview. Resolves the workspace file to a pre-signed URL
 *  (CLAUDE.md §5 — bytes never streamed directly from S3 to the client). */
export function ImageViewer({ fileId, fileName }: Props) {
  const [src, setSrc] = useState<string | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setSrc(null);
    setError(false);
    fileApi
      .downloadUrl(fileId)
      .then(({ url }) => {
        if (!cancelled) setSrc(url);
      })
      .catch(() => {
        if (!cancelled) setError(true);
      });
    return () => {
      cancelled = true;
    };
  }, [fileId]);

  return (
    <div className="flex flex-1 items-center justify-center overflow-auto bg-muted/40 p-3">
      {error ? (
        <div className="text-xs text-destructive">
          Bild konnte nicht geladen werden.
        </div>
      ) : src ? (
        <img
          src={src}
          alt={fileName}
          className="max-h-full max-w-full object-contain"
        />
      ) : (
        <div className="text-xs text-muted-foreground">Lädt …</div>
      )}
    </div>
  );
}
