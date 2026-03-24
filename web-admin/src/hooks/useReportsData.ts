import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { reports } from '../data/mockData';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type ReportAggregate = {
  id: string;
  dimension: string;
  period: string;
  metrics: Record<string, number>;
  createdAt: string;
};

type ReportsState = {
  data: ReportAggregate[];
  loading: boolean;
  error?: string;
  isMock: boolean;
};

type ApiReportingAggregate = {
  id: string;
  dimension: string;
  period_start: string;
  metrics: Record<string, number>;
  created_at: string;
};

const fallbackReports: ReportAggregate[] = reports.map((item) => ({
  id: item.id,
  dimension: item.dimension,
  period: item.period,
  metrics: item.metrics,
  createdAt: new Date().toISOString(),
}));

const mapAggregate = (aggregate: ApiReportingAggregate): ReportAggregate => ({
  id: aggregate.id,
  dimension: aggregate.dimension,
  period: aggregate.period_start,
  metrics: aggregate.metrics,
  createdAt: aggregate.created_at,
});

export const useReportsData = (dimension = 'category'): ReportsState => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.reportingAggregates(baseUrl, dimension),
    enabled,
    queryFn: async () => {
      const body = await adminGetJson<{ data?: ApiReportingAggregate[] } | ApiReportingAggregate[]>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/reporting/aggregates',
        {
          dimension,
          page_size: 6,
        },
      );
      const rows = Array.isArray((body as { data?: ApiReportingAggregate[] }).data)
        ? (body as { data: ApiReportingAggregate[] }).data
        : (body as ApiReportingAggregate[]);
      return rows.map(mapAggregate);
    },
  });

  if (!enabled) {
    return { data: fallbackReports, loading: false, isMock: true };
  }
  if (query.isError) {
    return {
      data: fallbackReports,
      loading: false,
      error: query.error instanceof Error ? query.error.message : 'Failed to fetch reporting aggregates',
      isMock: true,
    };
  }
  return {
    data: query.data ?? fallbackReports,
    loading: query.isLoading,
    isMock: false,
  };
};
