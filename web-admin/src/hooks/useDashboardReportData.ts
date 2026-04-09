import { useQuery } from '@tanstack/react-query';
import { AdminApiError, adminGetJson, type AdminApiContext } from '../api/adminClient';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type TopEntry = {
  key: string;
  doc_count: number;
};

export type TopBandwidthEntry = {
  key: string;
  doc_count: number;
  bandwidth_bytes: number;
};

export type HourlyUsageBucket = {
  timestamp: string;
  total_requests: number;
  blocked_requests: number;
  bandwidth_bytes: number;
};

export type DashboardOverview = {
  total_requests: number;
  allow_requests: number;
  blocked_requests: number;
  block_rate: number;
  unique_clients: number;
  total_bandwidth_bytes: number;
};

export type DashboardCoverage = {
  total_docs: number;
  client_ip_docs: number;
  domain_docs: number;
  network_bytes_docs: number;
};

export type DashboardReport = {
  range: string;
  bucket_interval: string;
  overview: DashboardOverview;
  hourly_usage: HourlyUsageBucket[];
  top_domains: TopEntry[];
  top_blocked_domains: TopEntry[];
  top_blocked_requesters: TopEntry[];
  top_clients_by_bandwidth: TopBandwidthEntry[];
  coverage: DashboardCoverage;
};

type DashboardReportState = {
  data?: DashboardReport;
  loading: boolean;
  error?: string;
  isMock: boolean;
  updatedAt?: number;
  refresh: () => Promise<unknown>;
};

export const useDashboardReportData = (
  range = '24h',
  topN = 10,
  bucket?: string,
  refreshIntervalMs = 0,
): DashboardReportState => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.reportingDashboard(baseUrl, range, topN, bucket),
    enabled,
    refetchInterval: refreshIntervalMs > 0 ? refreshIntervalMs : false,
    refetchIntervalInBackground: false,
    queryFn: async () =>
      adminGetJson<DashboardReport>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/reporting/dashboard',
        {
          range,
          top_n: topN,
          bucket: bucket || undefined,
        },
      ),
  });

  if (!enabled) {
    return { data: undefined, loading: false, isMock: true, updatedAt: undefined, refresh: query.refetch };
  }
  if (query.isError) {
    const message =
      query.error instanceof AdminApiError && (query.error.status === 401 || query.error.status === 403)
        ? 'Session expired. Please sign in again.'
        : query.error instanceof Error
          ? query.error.message
          : 'Failed to fetch dashboard report';
    return {
      data: undefined,
      loading: false,
      error: message,
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
