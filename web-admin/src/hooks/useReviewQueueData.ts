import { useEffect, useState } from 'react';
import { reviewQueue } from '../data/mockData';
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

export const useReviewQueueData = (): ReviewQueueState => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const [state, setState] = useState<ReviewQueueState>({
    data: fallbackRows,
    loading: Boolean(canCallApi),
    isMock: !canCallApi,
  });

  useEffect(() => {
    if (!baseUrl || !canCallApi) {
      setState({ data: fallbackRows, loading: false, isMock: true });
      return;
    }

    let cancelled = false;
    const controller = new AbortController();

    const fetchQueue = async () => {
      setState((prev) => ({ ...prev, loading: true, error: undefined }));
      try {
        const resp = await fetch(`${baseUrl}/api/v1/review-queue`, {
          headers,
          signal: controller.signal,
        });
        if (!resp.ok) {
          throw new Error(`Request failed (${resp.status})`);
        }
        const body = (await resp.json()) as ApiReviewRecord[];
        if (!cancelled) {
          setState({ data: body.map(mapReviewRecord), loading: false, isMock: false });
        }
      } catch (err) {
        if (controller.signal.aborted || cancelled) {
          return;
        }
        setState({
          data: fallbackRows,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to fetch review queue',
          isMock: true,
        });
      }
    };

    fetchQueue();
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [baseUrl, canCallApi, headers]);

  return state;
};
