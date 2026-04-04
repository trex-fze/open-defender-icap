import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import type { CursorMeta, CursorPaged } from '../types/pagination';
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
  effective_action?: string;
  effective_decision_source?: string;
  confidence?: number;
  status: string;
  updated_at: string;
};

const emptyMeta: CursorMeta = {
  limit: 50,
  has_more: false,
};

export const useClassificationsData = (
  state: ClassificationStateFilter,
  q: string,
  cursor: string | undefined,
  limit: number,
) => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.classifications(baseUrl, state, q, cursor ?? '', limit),
    enabled,
    queryFn: async () =>
      adminGetJson<CursorPaged<ClassificationRecord>>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/classifications',
        { state, q, limit, cursor },
      ),
  });

  return {
    data: query.data?.data ?? [],
    meta: query.data?.meta ?? emptyMeta,
    loading: query.isLoading,
    error: query.error instanceof Error ? query.error.message : undefined,
    refresh: query.refetch,
    canCallApi,
    isMock: !enabled,
  } as const;
};
