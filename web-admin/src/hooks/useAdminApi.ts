import { useMemo } from 'react';
import { useAuth } from '../context/AuthContext';

const TOKEN_MODE = (import.meta.env.VITE_ADMIN_TOKEN_MODE ?? 'auto').trim().toLowerCase();

const isLikelyJwt = (token: string): boolean => {
  const parts = token.split('.');
  return parts.length === 3 && parts.every((part) => part.length > 0);
};

const localhostFallback = () => {
  if (typeof window !== 'undefined') {
    return window.location.origin;
  }
  return 'http://localhost:19001';
};

const resolveBaseUrl = () => {
  const runtimeOverride =
    typeof window !== 'undefined'
      ? (window as Window & { __OD_ADMIN_API_URL__?: string }).__OD_ADMIN_API_URL__
      : undefined;
  const envUrl = (import.meta.env.VITE_ADMIN_API_URL ?? '').trim();
  const fallbackEnv = (import.meta.env.VITE_ADMIN_API_FALLBACK ?? '').trim();
  const candidate = envUrl || fallbackEnv || localhostFallback();
  return (runtimeOverride ?? candidate).trim();
};

export const useAdminApi = () => {
  const { tokens, expireSession } = useAuth();
  const baseUrl = resolveBaseUrl();
  const accessToken = tokens?.accessToken?.trim();
  const canCallApi = Boolean(baseUrl && accessToken);

  const headers = useMemo<HeadersInit>(() => {
    const base: HeadersInit = {
      Accept: 'application/json',
    };
    if (accessToken) {
      const useBearer =
        TOKEN_MODE === 'bearer' || (TOKEN_MODE === 'auto' && isLikelyJwt(accessToken));
      if (useBearer) {
        base.Authorization = `Bearer ${accessToken}`;
      } else {
        base['X-Admin-Token'] = accessToken;
      }
    }
    return base;
  }, [accessToken]);

  return useMemo(
    () => ({
      baseUrl,
      accessToken,
      canCallApi,
      headers,
      onUnauthorized: () => expireSession('Session expired. Please sign in again.'),
    }),
    [baseUrl, accessToken, canCallApi, headers, expireSession],
  );
};
