import {
  Bot,
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileEdit,
  FileImage,
  FilePen,
  FilePlus,
  FileSearch,
  FileSignature,
  FileSpreadsheet,
  FileStack,
  FileText,
  FileType,
  FileVideo,
  Folder,
  FolderOpen,
  FolderSearch,
  MessageCircleQuestion,
  MessagesSquare,
  Search,
  Sheet,
  Wrench,
  type LucideIcon,
} from "lucide-react";

const REGISTRY: Record<string, LucideIcon> = {
  Bot,
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileEdit,
  FileImage,
  FilePen,
  FilePlus,
  FileSearch,
  FileSignature,
  FileSpreadsheet,
  FileStack,
  FileText,
  FileType,
  FileVideo,
  Folder,
  FolderOpen,
  FolderSearch,
  MessageCircleQuestion,
  MessagesSquare,
  Search,
  Sheet,
  Wrench,
};

type Props = {
  name: string | null | undefined;
  fallback?: LucideIcon;
  className?: string;
};

/** Renders a Lucide icon resolved from a stored string name (e.g. skill/agent icon fields). */
export function DynamicIcon({ name, fallback = Bot, className }: Props) {
  const Icon = (name && REGISTRY[name]) || fallback;
  return <Icon className={className} />;
}
