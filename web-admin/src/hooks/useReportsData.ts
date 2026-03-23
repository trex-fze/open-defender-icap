import { useEffect, useState } from 'react';
import { reports } from '../data/mockData';
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
  const [state, setState] = useState<ReportsState>({
    data: fallbackReports,
    loading: Boolean(canCallApi),
    isMock: !canCallApi,
  });

  useEffect(() => {
    if (!baseUrl || !canCallApi) {
      setState({ data: fallbackReports, loading: false, isMock: true });
      return;
    }

    let cancelled = false;
    const controller = new AbortController();

    const fetchReports = async () => {
      setState((prev) => ({ ...prev, loading: true, error: undefined }));
      try {
        const url = new URL('/api/v1/reporting/aggregates', baseUrl);
        url.searchParams.set('dimension', dimension);
        url.searchParams.set('page_size', '6');
        const resp = await fetch(url, { headers, signal: controller.signal });
        if (!resp.ok) {
          throw new Error(`Request failed (${resp.status})`);
        }
        const body = await resp.json();
        const data = Array.isArray(body?.data) ? body.data : body;
        if (!cancelled) {
          setState({ data: data.map(mapAggregate), loading: false, isMock: false });
        }
      } catch (err) {
        if (controller.signal.aborted || cancelled) {
          return;
        }
        setState({
          data: fallbackReports,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to fetch reporting aggregates',
          isMock: true,
        });
      }
    };

    fetchReports();
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [baseUrl, canCallApi, dimension, headers]);

  return state;
};
