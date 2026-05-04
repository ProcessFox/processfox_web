import { useEffect, useRef, useState } from "react";
import { HelpCircle, Send } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import type { PendingQuestion } from "@/types/chat";

type Props = {
  question: PendingQuestion;
  busy?: boolean;
  onRespond: (answer: string) => void;
};

export function AskUserCard({ question, busy, onRespond }: Props) {
  const [answer, setAnswer] = useState("");
  const ref = useRef<HTMLTextAreaElement>(null);

  // Auto-focus the textarea when the card mounts so the user can start
  // typing immediately without having to click into it.
  useEffect(() => {
    ref.current?.focus();
  }, [question.questionId]);

  const submit = () => {
    const trimmed = answer.trim();
    if (trimmed.length === 0) return;
    onRespond(trimmed);
  };

  return (
    <div className="flex flex-col gap-2 rounded-md border border-sky-500/40 bg-sky-500/10 p-3 text-xs text-sky-900 dark:text-sky-200">
      <div className="flex items-center gap-2">
        <HelpCircle className="h-3.5 w-3.5" />
        <span className="text-sm font-medium">Frage vom Agenten</span>
      </div>

      <div className="rounded-sm border border-sky-500/30 bg-background/60 px-2 py-1.5 text-sm whitespace-pre-wrap">
        {question.question}
      </div>

      <Textarea
        ref={ref}
        value={answer}
        onChange={(e) => setAnswer(e.target.value)}
        placeholder="Antwort eingeben…"
        disabled={busy}
        rows={3}
        className="min-h-16 text-xs"
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            submit();
          }
        }}
      />

      <div className="flex items-center justify-between gap-2 pt-1">
        <span className="text-xs opacity-70">⌘/Ctrl + Enter senden</span>
        <Button
          size="sm"
          onClick={submit}
          disabled={busy || answer.trim().length === 0}
          className="gap-1.5"
        >
          <Send className="h-3.5 w-3.5" />
          Antwort senden
        </Button>
      </div>
    </div>
  );
}
