import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import type { CursorPaged } from '../types/pagination';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

type PendingRecord = {
  normalized_key: string;
};

type ProviderSummary = {
  name: string;
};

export type OpsSnapshot = {
  pendingCount: number;
  llmProviderNames: string[];
  source: 'live' | 'partial' | 'mock';
};

const PROVIDERS_URL = (import.meta.env.VITE_LLM_PROVIDERS_URL ?? '').trim();

const DEFAULT_SNAPSHOT: OpsSnapshot = {
  pendingCount: 0,
  llmProviderNames: [],
  source: 'mock',
};

export const useOpsStatus = (refreshIntervalMs = 0) => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.opsStatus(baseUrl, PROVIDERS_URL || 'admin-api'),
    enabled,
    refetchInterval: refreshIntervalMs > 0 ? refreshIntervalMs : false,
    refetchIntervalInBackground: false,
    queryFn: async (): Promise<OpsSnapshot> => {
      const pending = await adminGetJson<CursorPaged<PendingRecord>>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/classifications/pending',
        { limit: 500 },
      );

      try {
        if (!PROVIDERS_URL) {
          const providers = await adminGetJson<ProviderSummary[]>(
            { baseUrl, canCallApi, headers } as AdminApiContext,
            '/api/v1/ops/llm/providers',
          );
          return {
            pendingCount: pending.data.length,
            llmProviderNames: providers.map((item) => item.name),
            source: 'live',
          };
        }

        const resp = await fetch(PROVIDERS_URL);
        if (!resp.ok) {
          return {
            pendingCount: pending.data.length,
            llmProviderNames: [],
            source: 'partial',
          };
        }
        const providers = (await resp.json()) as ProviderSummary[];
        return {
          pendingCount: pending.data.length,
          llmProviderNames: providers.map((item) => item.name),
          source: 'live',
        };
      } catch {
        return {
          pendingCount: pending.data.length,
          llmProviderNames: [],
          source: 'partial',
        };
      }
    },
  });

  if (!enabled) {
    return { data: DEFAULT_SNAPSHOT, loading: false, error: undefined, updatedAt: undefined } as const;
  }

  if (query.isError) {
    return {
      data: DEFAULT_SNAPSHOT,
      loading: false,
      error: query.error instanceof Error ? query.error.message : 'Failed to fetch operations status',
      updatedAt: query.dataUpdatedAt,
    } as const;
  }

  return {
    data: query.data ?? DEFAULT_SNAPSHOT,
    loading: query.isLoading,
    error: undefined,
    updatedAt: query.dataUpdatedAt,
  } as const;
};
