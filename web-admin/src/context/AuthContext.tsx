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
  login: (profile?: Partial<UserProfile>, options?: { tokens?: AuthTokens }) => void;
  logout: () => void;
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

const readStoredTokens = (): AuthTokens | null => {
  if (typeof window !== 'undefined') {
    const raw = window.localStorage.getItem(TOKEN_STORAGE_KEY);
    if (raw) {
      try {
        const parsed = JSON.parse(raw) as AuthTokens;
        if (parsed?.accessToken) {
          return parsed;
        }
      } catch (err) {
        console.warn('Failed to parse stored admin tokens', err);
      }
    }
  }
  return ENV_BOOTSTRAP_TOKEN ? { accessToken: ENV_BOOTSTRAP_TOKEN } : null;
};

export const AuthProvider = ({ children }: { children: ReactNode }) => {
  const [user, setUser] = useState<UserProfile | null>(defaultUser);
  const [tokens, setTokens] = useState<AuthTokens | null>(() => readStoredTokens());

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
      hasRole,
      hasAnyRole,
      login: (profile, options) => {
        setUser({
          name: profile?.name ?? defaultUser.name,
          email: profile?.email ?? defaultUser.email,
          roles: profile?.roles ?? defaultUser.roles,
        });
        if (options?.tokens) {
          setTokens(options.tokens);
        } else if (!tokens) {
          setTokens(readStoredTokens());
        }
      },
      logout: () => {
        setUser(null);
        setTokens(null);
      },
      setTokens,
    };
  }, [tokens, user]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};

export const useAuth = () => {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error('useAuth must be used within AuthProvider');
  }
  return ctx;
};
