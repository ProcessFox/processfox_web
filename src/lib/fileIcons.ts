import {
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileImage,
  FileSpreadsheet,
  FileText,
  FileType,
  FileVideo,
  Monitor,
  type LucideIcon,
} from "lucide-react";

const EXT_ICONS: Record<string, LucideIcon> = {
  // Documents
  docx: FileText,
  doc: FileText,
  odt: FileText,
  rtf: FileText,
  md: FileText,
  markdown: FileText,
  txt: FileText,
  text: FileText,
  log: FileText,
  // Spreadsheets
  xlsx: FileSpreadsheet,
  xls: FileSpreadsheet,
  csv: FileSpreadsheet,
  ods: FileSpreadsheet,
  tsv: FileSpreadsheet,
  // PDF
  pdf: FileType,
  // Presentations
  pptx: Monitor,
  ppt: Monitor,
  odp: Monitor,
  key: Monitor,
  // Code / structured data
  json: FileCode,
  yaml: FileCode,
  yml: FileCode,
  toml: FileCode,
  xml: FileCode,
  html: FileCode,
  css: FileCode,
  ts: FileCode,
  tsx: FileCode,
  js: FileCode,
  jsx: FileCode,
  py: FileCode,
  rs: FileCode,
  go: FileCode,
  java: FileCode,
  sh: FileCode,
  // Images
  png: FileImage,
  jpg: FileImage,
  jpeg: FileImage,
  gif: FileImage,
  webp: FileImage,
  svg: FileImage,
  bmp: FileImage,
  ico: FileImage,
  // Audio
  mp3: FileAudio,
  wav: FileAudio,
  flac: FileAudio,
  m4a: FileAudio,
  ogg: FileAudio,
  // Video
  mp4: FileVideo,
  mov: FileVideo,
  avi: FileVideo,
  webm: FileVideo,
  mkv: FileVideo,
  // Archives
  zip: FileArchive,
  tar: FileArchive,
  gz: FileArchive,
  "7z": FileArchive,
  rar: FileArchive,
};

export function iconForFile(name: string): LucideIcon {
  const lower = name.toLowerCase();
  const lastDot = lower.lastIndexOf(".");
  if (lastDot < 0 || lastDot === lower.length - 1) return File;
  const ext = lower.slice(lastDot + 1);
  return EXT_ICONS[ext] ?? File;
}
