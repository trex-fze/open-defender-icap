import { createContext, ReactNode, useContext, useEffect, useMemo, useState } from 'react';

export type Role =
  | 'policy-admin'
  | 'policy-editor'
  | 'policy-viewer'
  | 'review-approver'
  | 'auditor';

export type UserProfile = {
  name: string;
  email: string;
  roles: Role[];
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
  hasRole: (role: Role) => boolean;
  hasAnyRole: (roles?: Role[]) => boolean;
  setTokens: (tokens: AuthTokens | null) => void;
};

export const AuthContext = createContext<AuthContextValue | undefined>(undefined);

const defaultUser: UserProfile = {
  name: 'Avery Quinn',
  email: 'avery@example.com',
  roles: ['policy-admin', 'policy-viewer', 'review-approver'],
};

const TOKEN_STORAGE_KEY = 'od.admin.tokens';
const ENV_BOOTSTRAP_TOKEN = (import.meta.env.VITE_ADMIN_TOKEN ?? '').trim();
const DEFAULT_TOKEN_TTL_MS = 60 * 60 * 1000;

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

const readStoredTokens = (): AuthTokens | null => {
  if (ENV_BOOTSTRAP_TOKEN) {
    return normalizeToken({ accessToken: ENV_BOOTSTRAP_TOKEN });
  }

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
  const [user, setUser] = useState<UserProfile | null>(defaultUser);
  const [tokens, setTokens] = useState<AuthTokens | null>(() => readStoredTokens());
  const [authNotice, setAuthNotice] = useState<string | undefined>();

  useEffect(() => {
    if (!tokens?.accessToken || !tokens.expiresAt) return;
    if (isExpired(tokens)) {
      setUser(null);
      setTokens(null);
      setAuthNotice('Session expired. Please sign in again.');
      return;
    }

    const timeoutMs = tokens.expiresAt - Date.now();
    const timer = window.setTimeout(() => {
      setUser(null);
      setTokens(null);
      setAuthNotice('Session expired. Please sign in again.');
    }, timeoutMs);

    return () => window.clearTimeout(timer);
  }, [tokens]);

  useEffect(() => {
    if (typeof window === 'undefined') return;
    if (tokens?.accessToken) {
      window.localStorage.setItem(TOKEN_STORAGE_KEY, JSON.stringify(tokens));
    } else {
      window.localStorage.removeItem(TOKEN_STORAGE_KEY);
    }
  }, [tokens]);

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
        setUser({
          name: profile?.name ?? defaultUser.name,
          email: profile?.email ?? defaultUser.email,
          roles: profile?.roles ?? defaultUser.roles,
        });
        if (options?.tokens) {
          setTokens(normalizeToken(options.tokens));
          setAuthNotice(undefined);
        } else if (!tokens) {
          setTokens(readStoredTokens());
          setAuthNotice(undefined);
        }
      },
      logout: () => {
        setUser(null);
        setTokens(null);
        setAuthNotice(undefined);
      },
      clearAuthNotice: () => setAuthNotice(undefined),
      setTokens,
    };
  }, [tokens, user, authNotice]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};

export const useAuth = () => {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error('useAuth must be used within AuthProvider');
  }
  return ctx;
};
