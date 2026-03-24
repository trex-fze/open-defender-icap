import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { reviewQueue } from '../data/mockData';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type ReviewQueueRow = {
  id: string;
  key: string;
  status: string;
  risk: string;
  sla: string;
  assignedTo?: string;
};

type ReviewQueueState = {
  data: ReviewQueueRow[];
  loading: boolean;
  error?: string;
  isMock: boolean;
};

type ApiReviewRecord = {
  id: string;
  normalized_key: string;
  status: string;
  assigned_to?: string;
  request_metadata?: Record<string, any>;
  created_at?: string;
};

const fallbackRows: ReviewQueueRow[] = reviewQueue.map((item) => ({
  id: item.id,
  key: item.key,
  status: item.status,
  risk: item.risk,
  sla: item.sla,
}));

const deriveRisk = (metadata?: Record<string, any>): string => {
  if (!metadata) return 'medium';
  return (
    metadata.risk?.toString() ||
    metadata.risk_level?.toString() ||
    metadata.segment_risk?.toString() ||
    'medium'
  );
};

const deriveSla = (createdAt?: string): string => {
  if (!createdAt) return '—';
  const created = Date.parse(createdAt);
  if (Number.isNaN(created)) return '—';
  const diffMinutes = Math.max(0, Math.round((Date.now() - created) / 60000));
  if (diffMinutes >= 60) {
    const hours = Math.floor(diffMinutes / 60);
    return `${hours}h`;
  }
  return `${diffMinutes}m`;
};

const mapReviewRecord = (record: ApiReviewRecord): ReviewQueueRow => ({
  id: record.id,
  key: record.normalized_key,
  status: record.status,
  risk: deriveRisk(record.request_metadata),
  sla: deriveSla(record.created_at),
  assignedTo: record.assigned_to,
});

export const useReviewQueueData = () => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.reviewQueue(baseUrl),
    enabled,
    queryFn: async () => {
      const body = await adminGetJson<ApiReviewRecord[]>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/review-queue',
      );
      return body.map(mapReviewRecord);
    },
  });

  const refresh = async () => {
    await query.refetch();
  };

  const state: ReviewQueueState = !enabled
    ? { data: fallbackRows, loading: false, isMock: true }
    : query.isError
      ? {
          data: fallbackRows,
          loading: false,
          error: query.error instanceof Error ? query.error.message : 'Failed to fetch review queue',
          isMock: true,
        }
      : {
          data: query.data ?? fallbackRows,
          loading: query.isLoading,
          isMock: false,
        };

  return {
    ...state,
    refresh,
    canCallApi,
  } as const;
};
