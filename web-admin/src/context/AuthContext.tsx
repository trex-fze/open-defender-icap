import { createContext, ReactNode, useContext, useMemo, useState } from 'react';

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

type AuthContextValue = {
  user: UserProfile | null;
  login: (profile?: Partial<UserProfile>) => void;
  logout: () => void;
  hasRole: (role: Role) => boolean;
  hasAnyRole: (roles?: Role[]) => boolean;
};

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

const defaultUser: UserProfile = {
  name: 'Avery Quinn',
  email: 'avery@example.com',
  roles: ['policy-admin', 'policy-viewer', 'review-approver'],
};

export const AuthProvider = ({ children }: { children: ReactNode }) => {
  const [user, setUser] = useState<UserProfile | null>(defaultUser);

  const value = useMemo<AuthContextValue>(() => {
    const hasRole = (role: Role) => Boolean(user?.roles.includes(role));
    const hasAnyRole = (roles?: Role[]) => {
      if (!roles || roles.length === 0) return Boolean(user);
      return roles.some((role) => hasRole(role));
    };

    return {
      user,
      hasRole,
      hasAnyRole,
      login: (profile) =>
        setUser({
          name: profile?.name ?? defaultUser.name,
          email: profile?.email ?? defaultUser.email,
          roles: profile?.roles ?? defaultUser.roles,
        }),
      logout: () => setUser(null),
    };
  }, [user]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};

export const useAuth = () => {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error('useAuth must be used within AuthProvider');
  }
  return ctx;
};
