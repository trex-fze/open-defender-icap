import { useMemo } from 'react';
import { useAuth } from '../context/AuthContext';
import { resolveAdminApiBase } from '../utils/adminApiBase';

const TOKEN_MODE = (import.meta.env.VITE_ADMIN_TOKEN_MODE ?? 'auto').trim().toLowerCase();

const isLikelyJwt = (token: string): boolean => {
  const parts = token.split('.');
  return parts.length === 3 && parts.every((part) => part.length > 0);
};

export const useAdminApi = () => {
  const { tokens, expireSession } = useAuth();
  const baseUrl = resolveAdminApiBase();
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
