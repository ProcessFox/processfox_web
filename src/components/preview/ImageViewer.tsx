import { convertFileSrc } from "@tauri-apps/api/core";

type Props = {
  filePath: string;
  fileName: string;
};

/** Image preview. Resolves the absolute file path to an `asset://` URL via
 *  Tauri's asset protocol — no bytes round-trip through Rust. The agent
 *  folder must already be in the asset-protocol scope (set up in
 *  `watch_agent_folder` on the Rust side). */
export function ImageViewer({ filePath, fileName }: Props) {
  const src = convertFileSrc(filePath);
  return (
    <div className="flex flex-1 items-center justify-center overflow-auto bg-muted/40 p-3">
      <img
        src={src}
        alt={fileName}
        className="max-h-full max-w-full object-contain"
      />
    </div>
  );
}
