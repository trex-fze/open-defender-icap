import { createContext, ReactNode, useCallback, useContext, useEffect, useMemo, useState } from 'react';

export type Role =
  | 'policy-admin'
  | 'policy-editor'
  | 'policy-viewer'
  | 'auditor';

export type UserProfile = {
  username?: string;
  name: string;
  email: string;
  roles: Role[];
  mustChangePassword?: boolean;
};

export type AuthTokens = {
  accessToken: string;
  refreshToken?: string;
  expiresAt?: number;
};

type AuthContextValue = {
  user: UserProfile | null;
  tokens: AuthTokens | null;
  authNotice?: string;
  login: (profile?: Partial<UserProfile>, options?: { tokens?: AuthTokens }) => void;
  logout: () => void;
  clearAuthNotice: () => void;
  expireSession: (notice?: string) => void;
  hasRole: (role: Role) => boolean;
  hasAnyRole: (roles?: Role[]) => boolean;
  setTokens: (tokens: AuthTokens | null) => void;
};

export const AuthContext = createContext<AuthContextValue | undefined>(undefined);

const TOKEN_STORAGE_KEY = 'od.admin.tokens';
const USER_STORAGE_KEY = 'od.admin.user';
const DEFAULT_TOKEN_TTL_MS = 60 * 60 * 1000;
const TOKEN_MODE = (import.meta.env.VITE_ADMIN_TOKEN_MODE ?? 'auto').trim().toLowerCase();

type WhoAmIResponse = {
  actor: string;
  roles: string[];
  username?: string | null;
  email?: string | null;
  display_name?: string | null;
  must_change_password?: boolean | null;
};

const knownRoles = new Set<Role>(['policy-admin', 'policy-editor', 'policy-viewer', 'auditor']);

const isExpired = (token?: AuthTokens | null): boolean => {
  if (!token?.expiresAt) return false;
  return Date.now() >= token.expiresAt;
};

const normalizeToken = (token: AuthTokens): AuthTokens => {
  if (token.expiresAt) {
    return token;
  }
  return {
    ...token,
    expiresAt: Date.now() + DEFAULT_TOKEN_TTL_MS,
  };
};

const normalizeRoles = (roles?: string[] | Role[]): Role[] => {
  if (!roles || roles.length === 0) return [];
  return roles.flatMap((role) => (knownRoles.has(role as Role) ? [role as Role] : []));
};

const isLikelyJwt = (token: string): boolean => {
  const parts = token.split('.');
  return parts.length === 3 && parts.every((part) => part.length > 0);
};

const resolveAdminApiBase = (): string => {
  const runtimeOverride =
    typeof window !== 'undefined'
      ? (window as Window & { __OD_ADMIN_API_URL__?: string }).__OD_ADMIN_API_URL__
      : undefined;
  const envUrl = (import.meta.env.VITE_ADMIN_API_URL ?? '').trim();
  const fallbackEnv = (import.meta.env.VITE_ADMIN_API_FALLBACK ?? '').trim();
  const fallbackBase =
    typeof window !== 'undefined'
      ? `${window.location.protocol}//${window.location.hostname}:19000`
      : 'http://localhost:19000';
  const candidate = (runtimeOverride ?? envUrl) || fallbackEnv || fallbackBase;
  return candidate.trim();
};

const readStoredUser = (): UserProfile | null => {
  if (typeof window === 'undefined') return null;
  const raw = window.localStorage.getItem(USER_STORAGE_KEY);
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<UserProfile>;
    if (!parsed.email || !parsed.name) {
      return null;
    }
    return {
      username: parsed.username,
      name: parsed.name,
      email: parsed.email,
      roles: normalizeRoles(parsed.roles),
      mustChangePassword: parsed.mustChangePassword === true,
    };
  } catch {
    return null;
  }
};

const readStoredTokens = (): AuthTokens | null => {
  if (typeof window !== 'undefined') {
    const raw = window.localStorage.getItem(TOKEN_STORAGE_KEY);
    if (raw) {
      try {
        const parsed = JSON.parse(raw) as AuthTokens;
        if (parsed?.accessToken) {
          const token = normalizeToken(parsed);
          if (isExpired(token)) {
            window.localStorage.removeItem(TOKEN_STORAGE_KEY);
            return null;
          }
          return token;
        }
      } catch (err) {
        console.warn('Failed to parse stored admin tokens', err);
      }
    }
  }
  return null;
};

export const AuthProvider = ({ children }: { children: ReactNode }) => {
  const [user, setUser] = useState<UserProfile | null>(() => readStoredUser());
  const [tokens, setTokens] = useState<AuthTokens | null>(() => readStoredTokens());
  const [authNotice, setAuthNotice] = useState<string | undefined>();

  const expireSession = useCallback((notice = 'Session expired. Please sign in again.') => {
    setUser(null);
    setTokens(null);
    setAuthNotice(notice);
  }, []);

  useEffect(() => {
    if (!tokens?.accessToken || !tokens.expiresAt) return;
    if (isExpired(tokens)) {
      expireSession();
      return;
    }

    const timeoutMs = tokens.expiresAt - Date.now();
    const timer = window.setTimeout(() => {
      expireSession();
    }, timeoutMs);

    return () => window.clearTimeout(timer);
  }, [expireSession, tokens]);

  useEffect(() => {
    if (typeof window === 'undefined') return;
    if (tokens?.accessToken) {
      window.localStorage.setItem(TOKEN_STORAGE_KEY, JSON.stringify(tokens));
    } else {
      window.localStorage.removeItem(TOKEN_STORAGE_KEY);
    }
  }, [tokens]);

  useEffect(() => {
    if (typeof window === 'undefined') return;
    if (user) {
      window.localStorage.setItem(USER_STORAGE_KEY, JSON.stringify(user));
    } else {
      window.localStorage.removeItem(USER_STORAGE_KEY);
    }
  }, [user]);

  useEffect(() => {
    if (!tokens?.accessToken) return;
    const controller = new AbortController();
    const refreshWhoAmI = async () => {
      try {
        const token = tokens.accessToken.trim();
        const headers: HeadersInit = { Accept: 'application/json' };
        const useBearer = TOKEN_MODE === 'bearer' || (TOKEN_MODE === 'auto' && isLikelyJwt(token));
        if (useBearer) {
          headers.Authorization = `Bearer ${token}`;
        } else {
          (headers as Record<string, string>)['X-Admin-Token'] = token;
        }

        const response = await fetch(new URL('/api/v1/iam/whoami', resolveAdminApiBase()).toString(), {
          method: 'GET',
          headers,
          signal: controller.signal,
        });

        if (!response.ok) {
          if (response.status === 401 || response.status === 403) {
            expireSession();
          }
          return;
        }

        const data = (await response.json()) as WhoAmIResponse;
        const effectiveEmail = data.email?.trim() || user?.email || data.actor;
        const effectiveName =
          data.display_name?.trim() || data.username?.trim() || user?.name || effectiveEmail;
        setUser({
          username: data.username ?? user?.username,
          name: effectiveName,
          email: effectiveEmail,
          roles: normalizeRoles(data.roles),
          mustChangePassword:
            data.must_change_password === undefined || data.must_change_password === null
              ? user?.mustChangePassword
              : data.must_change_password,
        });
      } catch {
        // no-op; keep existing session state on transient network errors
      }
    };

    refreshWhoAmI();
    return () => controller.abort();
  }, [expireSession, tokens?.accessToken]);

  const value = useMemo<AuthContextValue>(() => {
    const hasRole = (role: Role) => Boolean(user?.roles.includes(role));
    const hasAnyRole = (roles?: Role[]) => {
      if (!roles || roles.length === 0) return Boolean(user);
      return roles.some((role) => hasRole(role));
    };

    return {
      user,
      tokens,
      authNotice,
      hasRole,
      hasAnyRole,
      login: (profile, options) => {
        const email = profile?.email ?? user?.email ?? '';
        const username = profile?.username ?? user?.username;
        setUser({
          username,
          name: profile?.name ?? username ?? email,
          email,
          roles: normalizeRoles(profile?.roles ?? user?.roles),
          mustChangePassword: profile?.mustChangePassword ?? user?.mustChangePassword,
        });
        if (options?.tokens) {
          setTokens(normalizeToken(options.tokens));
          setAuthNotice(undefined);
        }
      },
      logout: () => {
        setUser(null);
        setTokens(null);
        setAuthNotice(undefined);
      },
      clearAuthNotice: () => setAuthNotice(undefined),
      expireSession,
      setTokens,
    };
  }, [tokens, user, authNotice, expireSession]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};

export const useAuth = () => {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error('useAuth must be used within AuthProvider');
  }
  return ctx;
};
