import { useMemo } from 'react';
import { useAuth } from '../context/AuthContext';

const TOKEN_MODE = (import.meta.env.VITE_ADMIN_TOKEN_MODE ?? 'auto').trim().toLowerCase();

const isLikelyJwt = (token: string): boolean => {
  const parts = token.split('.');
  return parts.length === 3 && parts.every((part) => part.length > 0);
};

const resolveBaseUrl = () => {
  const runtimeOverride =
    typeof window !== 'undefined'
      ? (window as Window & { __OD_ADMIN_API_URL__?: string }).__OD_ADMIN_API_URL__
      : undefined;
  return (runtimeOverride ?? import.meta.env.VITE_ADMIN_API_URL ?? '').trim();
};

export const useAdminApi = () => {
  const { tokens } = useAuth();
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

  return {
    baseUrl,
    accessToken,
    canCallApi,
    headers,
  };
};
