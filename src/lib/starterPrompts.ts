/**
 * Hand-curated example prompts shown as clickable chips in an empty chat.
 * Each entry is keyed by the skill name; if multiple of the active agent's
 * skills define a prompt, ChatPane picks the first ~4 it sees.
 */
export type StarterPrompt = {
  skill: string;
  text: string;
};

export const STARTER_PROMPTS: StarterPrompt[] = [
  {
    skill: "folder-search",
    text: 'Welche Dateien im Ordner enthalten das Wort „Vertrag"?',
  },
  {
    skill: "folder-search",
    text: "Liste alle PDFs im Ordner.",
  },
  {
    skill: "document-read",
    text: "Fasse mir die wichtigste Datei zusammen.",
  },
  {
    skill: "document-extend",
    text: "Notiere im Journal: heute habe ich ProcessFox eingerichtet.",
  },
  {
    skill: "document-create-docx",
    text: "Erstelle eine kurze Notiz zum heutigen Meeting.",
  },
  {
    skill: "document-edit",
    text: "Korrigiere alle Tippfehler in der Datei notes.md.",
  },
  {
    skill: "document-from-template",
    text: "Erstelle ein Angebot für eine fiktive Firma aus der Vorlage.",
  },
  {
    skill: "table-read",
    text: "Welche Werte stehen in der ersten Spalte der Excel-Datei?",
  },
  {
    skill: "table-update",
    text: "Setze die Marketing-Zeile in der Tabelle auf einen neuen Wert.",
  },
  {
    skill: "table-create",
    text: "Lege eine kleine Budget-Tabelle mit drei Posten an.",
  },
  {
    skill: "chat-context",
    text: "Was haben wir bisher besprochen?",
  },
];

/**
 * Pick up to `max` prompts whose skill is active on the agent. Falls back
 * to a small default set if the agent has no matching skills (so users with
 * a fresh "chat-only" agent still see something useful).
 */
export function pickStarterPrompts(
  activeSkillNames: string[],
  max = 4,
): StarterPrompt[] {
  const active = new Set(activeSkillNames);
  const matching = STARTER_PROMPTS.filter((p) => active.has(p.skill));
  if (matching.length === 0) {
    return STARTER_PROMPTS.slice(0, Math.min(max, 2));
  }
  return matching.slice(0, max);
}
