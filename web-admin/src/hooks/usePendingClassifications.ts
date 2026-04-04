import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import type { CursorMeta, CursorPaged } from '../types/pagination';
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
  meta: CursorMeta;
  loading: boolean;
  error?: string;
  isMock: boolean;
};

const fallback: PendingClassification[] = [];
const fallbackMeta: CursorMeta = { limit: 50, has_more: false };

export const usePendingClassifications = (
  status?: string,
  cursor?: string,
  limit = 50,
) => {
  const { baseUrl, canCallApi, headers } = useAdminApi();

  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.pendingClassifications(baseUrl, status ?? '', cursor ?? '', limit),
    enabled,
    queryFn: async () => {
      const body = await adminGetJson<
        CursorPaged<{
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
        { status, cursor, limit },
      );
      return {
        data: body.data.map((row) => ({
          normalizedKey: row.normalized_key,
          status: row.status,
          baseUrl: row.base_url,
          lastError: row.last_error,
          requestedAt: row.requested_at,
          updatedAt: row.updated_at,
        })),
        meta: body.meta,
      };
    },
  });

  const refresh = async () => {
    await query.refetch();
  };

  const state: PendingState = !enabled
    ? { data: fallback, meta: fallbackMeta, loading: false, isMock: true }
    : query.isError
      ? {
          data: fallback,
          meta: fallbackMeta,
          loading: false,
          error: query.error instanceof Error ? query.error.message : 'Failed to fetch pending classifications',
          isMock: true,
        }
      : {
          data: query.data?.data ?? fallback,
          meta: query.data?.meta ?? fallbackMeta,
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
