import { useCallback, useEffect, useState } from "react";

import {
  authApi,
  setAccessToken,
  setAuthCallbacks,
} from "@/lib/tauri";
import type { AuthSession, User } from "@/types/auth";

type State = {
  user: User | null;
  /** true, solange die Session beim Start wiederhergestellt wird. */
  loading: boolean;
};

/**
 * Passwordless-Auth-State. Access-Token lebt nur im Speicher (in `tauri.ts`);
 * beim Reload wird die Session über das httpOnly-Refresh-Cookie
 * wiederhergestellt. Magic-Link-Callback (`/auth/callback?token=…`) wird hier
 * eingelöst.
 */
export function useAuth() {
  const [state, setState] = useState<State>({ user: null, loading: true });

  const applySession = useCallback((s: AuthSession) => {
    setAccessToken(s.accessToken);
    setState({ user: s.user, loading: false });
  }, []);

  const clearSession = useCallback(() => {
    setAccessToken(null);
    setState({ user: null, loading: false });
  }, []);

  // Bridge darf Session bei transparentem Refresh aktualisieren / bei
  // endgültigem 401 invalidieren.
  useEffect(() => {
    setAuthCallbacks(
      (s) => setState({ user: s.user, loading: false }),
      () => clearSession(),
    );
  }, [clearSession]);

  // Bootstrap: Magic-Link-Callback einlösen, sonst Cookie-Refresh versuchen.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const url = new URL(window.location.href);
      const isCallback = url.pathname === "/auth/callback";
      const token = url.searchParams.get("token");

      if (isCallback && token) {
        try {
          const s = await authApi.verify(token);
          if (cancelled) return;
          applySession(s);
        } catch {
          if (!cancelled) clearSession();
        } finally {
          // Token aus der URL entfernen (History + Adresszeile säubern).
          window.history.replaceState({}, "", "/");
        }
        return;
      }

      const s = await authApi.refresh();
      if (cancelled) return;
      if (s) applySession(s);
      else clearSession();
    })();
    return () => {
      cancelled = true;
    };
  }, [applySession, clearSession]);

  const requestLogin = useCallback(
    (email: string) => authApi.requestLogin(email),
    [],
  );
  const requestRegister = useCallback(
    (email: string, inviteCode: string) =>
      authApi.requestRegister(email, inviteCode),
    [],
  );
  const logout = useCallback(async () => {
    await authApi.logout();
    clearSession();
  }, [clearSession]);

  return {
    user: state.user,
    loading: state.loading,
    isAuthenticated: state.user !== null,
    requestLogin,
    requestRegister,
    logout,
  } as const;
}
