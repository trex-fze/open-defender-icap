import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type ReportingStatus = {
  range: string;
  total_docs: number;
  action_docs: number;
  category_docs: number;
  domain_docs: number;
};

type ReportingStatusState = {
  data?: ReportingStatus;
  loading: boolean;
  error?: string;
};

export const useReportingStatus = (range = '24h'): ReportingStatusState => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.reportingStatus(baseUrl, range),
    enabled,
    queryFn: async () =>
      adminGetJson<ReportingStatus>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/reporting/status',
        { range },
      ),
  });

  if (!enabled) {
    return { data: undefined, loading: false };
  }
  if (query.isError) {
    return {
      data: undefined,
      loading: false,
      error: query.error instanceof Error ? query.error.message : 'Failed to fetch reporting status',
    };
  }

  return {
    data: query.data,
    loading: query.isLoading,
    error: undefined,
  };
};
