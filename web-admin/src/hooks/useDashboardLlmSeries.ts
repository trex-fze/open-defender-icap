import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type LlmSeriesPoint = {
  ts_ms: number;
  value: number;
};

export type LlmProviderSeries = {
  provider: string;
  success: LlmSeriesPoint[];
  failures: LlmSeriesPoint[];
  timeouts: LlmSeriesPoint[];
  non_retryable_400: LlmSeriesPoint[];
};

export type DashboardLlmSeries = {
  range: string;
  source: 'live' | 'partial' | 'unavailable';
  step_seconds: number;
  providers: LlmProviderSeries[];
  errors: string[];
};

type DashboardLlmSeriesState = {
  data?: DashboardLlmSeries;
  loading: boolean;
  error?: string;
  isMock: boolean;
  updatedAt?: number;
  refresh: () => Promise<unknown>;
};

export const useDashboardLlmSeries = (
  range = '24h',
  refreshIntervalMs = 0,
): DashboardLlmSeriesState => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.reportingOpsLlmSeries(baseUrl, range),
    enabled,
    refetchInterval: refreshIntervalMs > 0 ? refreshIntervalMs : false,
    refetchIntervalInBackground: false,
    queryFn: async () =>
      adminGetJson<DashboardLlmSeries>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/reporting/ops-llm-series',
        { range },
      ),
  });

  if (!enabled) {
    return { data: undefined, loading: false, isMock: true, updatedAt: undefined, refresh: query.refetch };
  }

  if (query.isError) {
    return {
      data: undefined,
      loading: false,
      error: query.error instanceof Error ? query.error.message : 'Failed to fetch LLM operations telemetry',
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
