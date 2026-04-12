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
  category_docs: number;
  category_mapped_domain_docs: number;
  category_mapped_ratio: number;
  network_bytes_docs: number;
};

export type DashboardReport = {
  range: string;
  bucket_interval: string;
  timezone?: string;
  overview: DashboardOverview;
  hourly_usage: HourlyUsageBucket[];
  top_domains: TopEntry[];
  top_categories: TopEntry[];
  top_categories_event?: TopEntry[];
  top_blocked_domains: TopEntry[];
  top_blocked_requesters: TopEntry[];
  top_clients_by_bandwidth: TopBandwidthEntry[];
  coverage: DashboardCoverage;
};

type DashboardReportPayload = Omit<DashboardReport, 'top_categories'> & {
  top_categories?: TopEntry[];
  top_categories_event?: TopEntry[];
  coverage?: Omit<DashboardCoverage, 'category_docs'> & {
    category_docs?: number;
    category_mapped_domain_docs?: number;
    category_mapped_ratio?: number;
  };
};

type TrafficCategoryPayload = {
  top_categories?: TopEntry[];
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
    queryFn: async () => {
      const context = { baseUrl, canCallApi, headers } as AdminApiContext;
      const report = await adminGetJson<DashboardReportPayload>(
        context,
        '/api/v1/reporting/dashboard',
        {
          range,
          top_n: topN,
          bucket: bucket || undefined,
        },
      );

      const normalizedReport: DashboardReport = {
        ...(report as Omit<DashboardReport, 'top_categories' | 'coverage'>),
        top_categories: report.top_categories ?? [],
        coverage: {
          total_docs: report.coverage?.total_docs ?? 0,
          client_ip_docs: report.coverage?.client_ip_docs ?? 0,
          domain_docs: report.coverage?.domain_docs ?? 0,
          category_docs: report.coverage?.category_docs ?? 0,
          category_mapped_domain_docs: report.coverage?.category_mapped_domain_docs ?? 0,
          category_mapped_ratio: report.coverage?.category_mapped_ratio ?? 0,
          network_bytes_docs: report.coverage?.network_bytes_docs ?? 0,
        },
      };

      if (Array.isArray(report.top_categories)) {
        return normalizedReport;
      }

      try {
        const traffic = await adminGetJson<TrafficCategoryPayload>(
          context,
          '/api/v1/reporting/traffic',
          {
            range,
            top_n: topN,
            bucket: bucket || undefined,
          },
        );
        return {
          ...normalizedReport,
          top_categories: traffic.top_categories ?? [],
        };
      } catch {
        return {
          ...normalizedReport,
          top_categories: [],
        };
      }
    },
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
