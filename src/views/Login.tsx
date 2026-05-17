import { useState, type FormEvent } from "react";
import { MailCheck } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

type Mode = "login" | "register";

type Props = {
  onRequestLogin: (email: string) => Promise<{ message: string }>;
  onRequestRegister: (
    email: string,
    inviteCode: string,
  ) => Promise<{ message: string }>;
};

/** Passwordless-Login: E-Mail (+ bei Registrierung Org-Code) → Magic-Link
 *  per Mail. Kein Passwortfeld. */
export function Login({ onRequestLogin, onRequestRegister }: Props) {
  const [mode, setMode] = useState<Mode>("login");
  const [email, setEmail] = useState("");
  const [inviteCode, setInviteCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sent, setSent] = useState<string | null>(null);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const res =
        mode === "login"
          ? await onRequestLogin(email.trim())
          : await onRequestRegister(email.trim(), inviteCode.trim());
      setSent(res.message);
    } catch (err) {
      setError(String((err as { message?: string })?.message ?? err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full w-full items-center justify-center bg-background p-6">
      <div className="w-full max-w-sm rounded-lg border border-border bg-surface p-6 shadow-subtle">
        <div className="mb-5 flex items-center gap-2">
          <img
            src="/icon.png"
            alt="ProcessFox"
            className="h-7 w-7 rounded-md"
          />
          <span className="text-lg font-semibold">ProcessFox</span>
        </div>

        {sent ? (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <MailCheck className="h-8 w-8 text-emerald-500" />
            <div className="text-sm font-medium">Prüfe deine E-Mails</div>
            <div className="text-xs text-muted-foreground">{sent}</div>
            <button
              onClick={() => {
                setSent(null);
                setError(null);
              }}
              className="mt-2 text-xs text-muted-foreground underline hover:text-foreground"
            >
              Andere Adresse verwenden
            </button>
          </div>
        ) : (
          <>
            <div className="mb-4 flex rounded-md border border-border p-0.5 text-xs">
              {(["login", "register"] as Mode[]).map((m) => (
                <button
                  key={m}
                  onClick={() => {
                    setMode(m);
                    setError(null);
                  }}
                  className={`flex-1 rounded-sm px-3 py-1.5 transition-colors ${
                    mode === m
                      ? "bg-primary/10 font-medium text-foreground"
                      : "text-muted-foreground hover:bg-accent"
                  }`}
                >
                  {m === "login" ? "Anmelden" : "Registrieren"}
                </button>
              ))}
            </div>

            <form onSubmit={handleSubmit} className="flex flex-col gap-3">
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="email" className="text-xs">
                  E-Mail
                </Label>
                <Input
                  id="email"
                  type="email"
                  required
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="du@beispiel.de"
                  autoFocus
                />
              </div>

              {mode === "register" && (
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="code" className="text-xs">
                    Einladungscode
                  </Label>
                  <Input
                    id="code"
                    required
                    value={inviteCode}
                    onChange={(e) => setInviteCode(e.target.value)}
                    placeholder="6-stelliger Org-Code"
                    maxLength={6}
                    className="font-mono uppercase tracking-widest"
                  />
                </div>
              )}

              {error && (
                <div className="rounded-md border border-destructive/40 bg-destructive/15 px-3 py-2 text-xs text-destructive">
                  {error}
                </div>
              )}

              <Button
                type="submit"
                disabled={busy}
                className="mt-1 w-full"
              >
                {busy
                  ? "Sende Link …"
                  : mode === "login"
                    ? "Login-Link senden"
                    : "Registrieren"}
              </Button>

              <p className="text-center text-[11px] text-muted-foreground">
                Wir senden dir einen einmaligen Anmeldelink per E-Mail —
                kein Passwort nötig.
              </p>
            </form>
          </>
        )}
      </div>
    </div>
  );
}
