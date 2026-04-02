import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type ClassificationStateFilter = 'all' | 'classified' | 'unclassified';

export type ClassificationRecord = {
  normalized_key: string;
  state: string;
  primary_category?: string;
  subcategory?: string;
  risk_level?: string;
  recommended_action?: string;
  confidence?: number;
  status: string;
  updated_at: string;
};

export const useClassificationsData = (state: ClassificationStateFilter, q: string) => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.classifications(baseUrl, state, q),
    enabled,
    queryFn: async () =>
      adminGetJson<ClassificationRecord[]>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/classifications',
        { state, q, limit: 250 },
      ),
  });

  return {
    data: query.data ?? [],
    loading: query.isLoading,
    error: query.error instanceof Error ? query.error.message : undefined,
    refresh: query.refetch,
    canCallApi,
    isMock: !enabled,
  } as const;
};
