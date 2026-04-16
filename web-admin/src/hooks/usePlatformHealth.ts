import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type PlatformHealthState = 'healthy' | 'degraded' | 'unreachable' | 'misconfigured' | 'unknown';

export type PlatformHealthComponent = {
  name: string;
  category: string;
  status: PlatformHealthState;
  checked_at_ms: number;
  latency_ms: number;
  endpoint?: string;
  http_status?: number;
  detail?: string;
  source: string;
};

export type PlatformHealthResponse = {
  source: string;
  checked_at_ms: number;
  overall_status: PlatformHealthState;
  summary: {
    total: number;
    healthy: number;
    degraded: number;
    unreachable: number;
    misconfigured: number;
    unknown: number;
  };
  components: PlatformHealthComponent[];
  errors: string[];
};

type PlatformHealthStateResult = {
  data?: PlatformHealthResponse;
  loading: boolean;
  error?: string;
  isMock: boolean;
  updatedAt?: number;
  refresh: () => Promise<unknown>;
};

export const usePlatformHealth = (refreshIntervalMs = 0): PlatformHealthStateResult => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.platformHealth(baseUrl),
    enabled,
    refetchInterval: refreshIntervalMs > 0 ? refreshIntervalMs : false,
    refetchIntervalInBackground: false,
    queryFn: async () =>
      adminGetJson<PlatformHealthResponse>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/ops/platform-health',
      ),
  });

  if (!enabled) {
    return { data: undefined, loading: false, error: undefined, isMock: true, updatedAt: undefined, refresh: query.refetch };
  }

  if (query.isError) {
    return {
      data: undefined,
      loading: false,
      error: query.error instanceof Error ? query.error.message : 'Failed to load platform health',
      isMock: true,
      updatedAt: query.dataUpdatedAt,
      refresh: query.refetch,
    };
  }

  return {
    data: query.data,
    loading: query.isLoading,
    error: undefined,
    isMock: false,
    updatedAt: query.dataUpdatedAt,
    refresh: query.refetch,
  };
};
