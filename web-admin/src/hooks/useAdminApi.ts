import { useMemo } from 'react';
import { useAuth } from '../context/AuthContext';

const API_BASE_URL = (import.meta.env.VITE_ADMIN_API_URL ?? '').trim();

export const useAdminApi = () => {
  const { tokens } = useAuth();
  const accessToken = tokens?.accessToken?.trim();
  const canCallApi = Boolean(API_BASE_URL && accessToken);

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
    baseUrl: API_BASE_URL,
    accessToken,
    canCallApi,
    headers,
  };
};
