import { useMemo } from 'react';
import { useAuth } from '../context/AuthContext';

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
      base.Authorization = `Bearer ${accessToken}`;
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
