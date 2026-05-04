import {
  FileEdit,
  FilePen,
  FilePlus,
  FileSearch,
  FileSignature,
  FileSpreadsheet,
  FileText,
  FileType,
  FolderOpen,
  MessageCircleQuestion,
  Wrench,
  type LucideIcon,
} from "lucide-react";

const TOOL_ICONS: Record<string, LucideIcon> = {
  list_folder: FolderOpen,
  read_file: FileText,
  grep_in_files: FileSearch,
  read_docx: FileText,
  read_xlsx_range: FileSpreadsheet,
  read_pdf: FileType,
  write_docx: FilePlus,
  write_xlsx: FilePlus,
  update_xlsx_cell: FilePen,
  append_to_md: FileSignature,
  append_to_docx: FileSignature,
  rewrite_file: FileEdit,
  ask_user: MessageCircleQuestion,
};

export function iconForTool(name: string): LucideIcon {
  return TOOL_ICONS[name] ?? Wrench;
}
