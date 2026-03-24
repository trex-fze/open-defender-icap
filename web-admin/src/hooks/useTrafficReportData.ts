import { useEffect, useState } from 'react';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
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
  const [state, setState] = useState<TrafficState>({
    data: undefined,
    loading: Boolean(canCallApi),
    isMock: !canCallApi,
  });

  useEffect(() => {
    if (!baseUrl || !canCallApi) {
      setState({ data: undefined, loading: false, isMock: true });
      return;
    }

    let cancelled = false;
    const controller = new AbortController();

    const fetchTraffic = async () => {
      setState((prev) => ({ ...prev, loading: true, error: undefined }));
      try {
        const data = await adminGetJson<TrafficReport>(
          { baseUrl, canCallApi, headers } as AdminApiContext,
          '/api/v1/reporting/traffic',
          {
            range,
            top_n: topN,
            bucket: bucket || undefined,
          },
          { signal: controller.signal },
        );
        if (!cancelled) {
          setState({ data, loading: false, isMock: false });
        }
      } catch (err) {
        if (controller.signal.aborted || cancelled) return;
        setState({
          data: undefined,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to fetch traffic report',
          isMock: true,
        });
      }
    };

    fetchTraffic();
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [baseUrl, canCallApi, headers, range, topN, bucket]);

  return state;
};
