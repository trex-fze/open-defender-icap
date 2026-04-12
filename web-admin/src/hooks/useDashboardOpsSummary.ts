import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type DashboardOpsProviderMetric = {
  provider: string;
  failures_5m: number | null;
  timeouts_5m: number | null;
  latency_p95_seconds?: number | null;
};

export type DashboardOpsSummary = {
  range: string;
  source: 'live' | 'partial' | 'unavailable';
  queue: {
    pending_age_p95_seconds?: number | null;
    llm_jobs_started_per_sec_10m?: number | null;
    llm_jobs_completed_per_sec_10m?: number | null;
    llm_dlq_growth_10m?: number | null;
    page_fetch_dlq_growth_10m?: number | null;
  };
  auth: {
    login_failures_10m?: number | null;
    lockouts_10m?: number | null;
    refresh_failures_10m?: number | null;
  };
  providers: DashboardOpsProviderMetric[];
  errors: string[];
};

type DashboardOpsState = {
  data?: DashboardOpsSummary;
  loading: boolean;
  error?: string;
  isMock: boolean;
  updatedAt?: number;
  refresh: () => Promise<unknown>;
};

export const useDashboardOpsSummary = (
  range = '24h',
  refreshIntervalMs = 0,
): DashboardOpsState => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.reportingOpsSummary(baseUrl, range),
    enabled,
    refetchInterval: refreshIntervalMs > 0 ? refreshIntervalMs : false,
    refetchIntervalInBackground: false,
    queryFn: async () =>
      adminGetJson<DashboardOpsSummary>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/reporting/ops-summary',
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
      error: query.error instanceof Error ? query.error.message : 'Failed to fetch operations telemetry',
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
