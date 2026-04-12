import { createContext, ReactNode, useContext, useEffect, useMemo, useState } from 'react';

export type ThemePreference = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

const THEME_STORAGE_KEY = 'od.theme.preference';

type ThemeContextValue = {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setPreference: (next: ThemePreference) => void;
};

const defaultContext: ThemeContextValue = {
  preference: 'system',
  resolvedTheme: 'dark',
  setPreference: () => {},
};

const ThemeContext = createContext<ThemeContextValue>(defaultContext);

const systemTheme = (): ResolvedTheme => {
  if (typeof window === 'undefined' || !window.matchMedia) {
    return 'dark';
  }
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
};

const readStoredPreference = (): ThemePreference => {
  if (typeof window === 'undefined') {
    return 'system';
  }
  const raw = window.localStorage.getItem(THEME_STORAGE_KEY);
  return raw === 'light' || raw === 'dark' || raw === 'system' ? raw : 'system';
};

const applyResolvedTheme = (theme: ResolvedTheme) => {
  if (typeof document === 'undefined') {
    return;
  }
  document.documentElement.setAttribute('data-theme', theme);
  document.documentElement.style.colorScheme = theme;
};

export const ThemeProvider = ({ children }: { children: ReactNode }) => {
  const [preference, setPreferenceState] = useState<ThemePreference>(readStoredPreference);
  const [resolvedTheme, setResolvedTheme] = useState<ResolvedTheme>(() =>
    readStoredPreference() === 'system' ? systemTheme() : readStoredPreference(),
  );

  useEffect(() => {
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(THEME_STORAGE_KEY, preference);
    }

    if (preference !== 'system') {
      setResolvedTheme(preference);
      return;
    }

    const media =
      typeof window !== 'undefined' && window.matchMedia
        ? window.matchMedia('(prefers-color-scheme: dark)')
        : null;

    const updateFromSystem = () => {
      setResolvedTheme(media?.matches ? 'dark' : 'light');
    };

    updateFromSystem();
    if (!media) {
      return;
    }

    media.addEventListener('change', updateFromSystem);
    return () => media.removeEventListener('change', updateFromSystem);
  }, [preference]);

  useEffect(() => {
    applyResolvedTheme(resolvedTheme);
  }, [resolvedTheme]);

  const value = useMemo(
    () => ({
      preference,
      resolvedTheme,
      setPreference: setPreferenceState,
    }),
    [preference, resolvedTheme],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
};

export const useTheme = () => useContext(ThemeContext);
