import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type PendingClassification = {
  normalizedKey: string;
  status: string;
  baseUrl?: string;
  lastError?: string;
  requestedAt: string;
  updatedAt: string;
};

type PendingState = {
  data: PendingClassification[];
  loading: boolean;
  error?: string;
  isMock: boolean;
};

const fallback: PendingClassification[] = [];

export const usePendingClassifications = () => {
  const { baseUrl, canCallApi, headers } = useAdminApi();

  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.pendingClassifications(baseUrl),
    enabled,
    queryFn: async () => {
      const body = await adminGetJson<
        Array<{
          normalized_key: string;
          status: string;
          base_url?: string;
          last_error?: string;
          requested_at: string;
          updated_at: string;
        }>
      >(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/classifications/pending',
      );
      return body.map((row) => ({
        normalizedKey: row.normalized_key,
        status: row.status,
        baseUrl: row.base_url,
        lastError: row.last_error,
        requestedAt: row.requested_at,
        updatedAt: row.updated_at,
      }));
    },
  });

  const refresh = async () => {
    await query.refetch();
  };

  const state: PendingState = !enabled
    ? { data: fallback, loading: false, isMock: true }
    : query.isError
      ? {
          data: fallback,
          loading: false,
          error: query.error instanceof Error ? query.error.message : 'Failed to fetch pending classifications',
          isMock: true,
        }
      : {
          data: query.data ?? fallback,
          loading: query.isLoading,
          isMock: false,
        };

  return {
    ...state,
    refresh,
    canCallApi,
    baseUrl,
    headers,
  } as const;
};
