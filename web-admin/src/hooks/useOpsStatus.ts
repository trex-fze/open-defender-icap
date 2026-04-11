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
  provider_type?: string;
  endpoint?: string;
  role?: string;
  health_status?: 'healthy' | 'degraded' | 'unreachable' | 'misconfigured' | 'unknown';
  health_checked_at_ms?: number;
  health_latency_ms?: number;
  health_http_status?: number;
  health_detail?: string;
};

export type OpsProviderStatus = {
  name: string;
  providerType: string;
  endpoint: string;
  role: string;
  healthStatus: 'healthy' | 'degraded' | 'unreachable' | 'misconfigured' | 'unknown';
  healthCheckedAtMs?: number;
  healthLatencyMs?: number;
  healthHttpStatus?: number;
  healthDetail?: string;
};

export type OpsSnapshot = {
  pendingCount: number;
  llmProviders: OpsProviderStatus[];
  llmProviderNames: string[];
  source: 'live' | 'partial' | 'mock';
};

const PROVIDERS_URL = (import.meta.env.VITE_LLM_PROVIDERS_URL ?? '').trim();

const DEFAULT_SNAPSHOT: OpsSnapshot = {
  pendingCount: 0,
  llmProviders: [],
  llmProviderNames: [],
  source: 'mock',
};

const mapProviders = (providers: ProviderSummary[]): OpsProviderStatus[] =>
  providers.map((item) => ({
    name: item.name,
    providerType: item.provider_type ?? 'unknown',
    endpoint: item.endpoint ?? '',
    role: item.role ?? 'unknown',
    healthStatus: item.health_status ?? 'unknown',
    healthCheckedAtMs: item.health_checked_at_ms,
    healthLatencyMs: item.health_latency_ms,
    healthHttpStatus: item.health_http_status,
    healthDetail: item.health_detail,
  }));

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
          const mappedProviders = mapProviders(providers);
          return {
            pendingCount: pending.data.length,
            llmProviders: mappedProviders,
            llmProviderNames: mappedProviders.map((item) => item.name),
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
        const mappedProviders = mapProviders(providers);
        return {
          pendingCount: pending.data.length,
          llmProviders: mappedProviders,
          llmProviderNames: mappedProviders.map((item) => item.name),
          source: 'live',
        };
      } catch {
        return {
          pendingCount: pending.data.length,
          llmProviders: [],
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
