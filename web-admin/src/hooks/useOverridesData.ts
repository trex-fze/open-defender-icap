import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { overrides } from '../data/mockData';
import type { CursorMeta, CursorPaged } from '../types/pagination';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type OverrideRow = {
  id: string;
  scopeType: string;
  scopeValue: string;
  scope: string;
  action: string;
  status: string;
  reason?: string;
  expiresAt?: string;
  expires: string;
};

type OverrideState = {
  data: OverrideRow[];
  meta: CursorMeta;
  loading: boolean;
  error?: string;
  isMock: boolean;
};

type ApiOverrideRecord = {
  id: string;
  scope_type: string;
  scope_value: string;
  action: string;
  status: string;
  reason?: string;
  expires_at?: string;
};

const fallbackRows: OverrideRow[] = overrides.map((item) => ({
  id: item.id,
  scopeType: item.scope.split(':')[0] ?? 'domain',
  scopeValue: item.scope.split(':').slice(1).join(':') || item.scope,
  scope: item.scope,
  action: item.action,
  status: item.status,
  reason: undefined,
  expiresAt: undefined,
  expires: item.expires,
}));
const fallbackMeta: CursorMeta = { limit: 50, has_more: false };

const mapOverride = (record: ApiOverrideRecord): OverrideRow => ({
  id: record.id,
  scopeType: record.scope_type,
  scopeValue: record.scope_value,
  scope:
    record.scope_type === 'domain'
      ? record.scope_value
      : `${record.scope_type}:${record.scope_value}`,
  action: record.action ?? 'unknown',
  status: record.status,
  reason: record.reason,
  expiresAt: record.expires_at,
  expires: record.expires_at
    ? new Date(record.expires_at).toLocaleString()
    : 'never',
});

export const useOverridesData = (cursor?: string, limit = 50) => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.overrides(baseUrl, cursor ?? '', limit),
    enabled,
    queryFn: async () => {
      const body = await adminGetJson<CursorPaged<ApiOverrideRecord>>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/overrides',
        { limit, cursor },
      );
      return {
        data: body.data.map(mapOverride),
        meta: body.meta,
      };
    },
  });

  const refresh = async () => {
    await query.refetch();
  };

  const state: OverrideState = !enabled
    ? { data: fallbackRows, meta: fallbackMeta, loading: false, isMock: true }
    : query.isError
      ? {
          data: fallbackRows,
          meta: fallbackMeta,
          loading: false,
          error: query.error instanceof Error ? query.error.message : 'Failed to fetch overrides',
          isMock: true,
        }
      : {
          data: query.data?.data ?? fallbackRows,
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
