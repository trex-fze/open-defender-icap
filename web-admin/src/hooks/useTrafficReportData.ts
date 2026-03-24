import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type TimeBucket = {
  key_as_string: string;
  doc_count: number;
};

export type ActionSeries = {
  action: string;
  buckets: TimeBucket[];
};

export type TopEntry = {
  key: string;
  doc_count: number;
};

export type TrafficReport = {
  range: string;
  bucket_interval: string;
  allow_block_trend: ActionSeries[];
  top_blocked_domains: TopEntry[];
  top_categories: TopEntry[];
};

type TrafficState = {
  data?: TrafficReport;
  loading: boolean;
  error?: string;
  isMock: boolean;
};

export const useTrafficReportData = (
  range = '24h',
  topN = 10,
  bucket?: string,
) => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.reportingTraffic(baseUrl, range, topN, bucket),
    enabled,
    queryFn: async () =>
      adminGetJson<TrafficReport>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/reporting/traffic',
        {
          range,
          top_n: topN,
          bucket: bucket || undefined,
        },
      ),
  });

  if (!enabled) {
    return { data: undefined, loading: false, isMock: true };
  }
  if (query.isError) {
    return {
      data: undefined,
      loading: false,
      error: query.error instanceof Error ? query.error.message : 'Failed to fetch traffic report',
      isMock: true,
    };
  }

  return {
    data: query.data,
    loading: query.isLoading,
    error: undefined,
    isMock: false,
  };
};
