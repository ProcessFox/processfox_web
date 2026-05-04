/** Classify a file by extension into the preview viewer that should render
 *  it. We lower-case the extension and accept the dot or no-dot form. */

export type PreviewKind =
  | "markdown"
  | "text"
  | "pdf"
  | "image"
  | "docx"
  | "xlsx"
  | "pptx"
  | "unsupported";

const TEXT_EXTS = new Set([
  "txt",
  "log",
  "json",
  "yaml",
  "yml",
  "toml",
  "csv",
  "tsv",
  "ini",
  "env",
  "xml",
  "html",
  "css",
  "ts",
  "tsx",
  "js",
  "jsx",
  "py",
  "rs",
  "go",
  "java",
  "sh",
]);

const IMAGE_EXTS = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "svg",
  "bmp",
  "ico",
]);

export function previewKindForName(name: string): PreviewKind {
  const lower = name.toLowerCase();
  const dot = lower.lastIndexOf(".");
  if (dot < 0 || dot === lower.length - 1) return "unsupported";
  const ext = lower.slice(dot + 1);

  if (ext === "md" || ext === "markdown") return "markdown";
  if (TEXT_EXTS.has(ext)) return "text";
  if (ext === "pdf") return "pdf";
  if (IMAGE_EXTS.has(ext)) return "image";
  if (ext === "docx") return "docx";
  if (ext === "xlsx") return "xlsx";
  if (ext === "pptx") return "pptx";
  return "unsupported";
}
