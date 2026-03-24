import { useEffect, useState } from 'react';
import { useAdminApi } from './useAdminApi';

export type PendingClassification = {
  normalizedKey: string;
  status: string;
  baseUrl?: string;
  lastError?: string;
  requestedAt: string;
  updatedAt: string;
};

type PendingState = {
  data: PendingClassification[];
  loading: boolean;
  error?: string;
  isMock: boolean;
};

const fallback: PendingClassification[] = [];

export const usePendingClassifications = () => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const [state, setState] = useState<PendingState>({
    data: fallback,
    loading: Boolean(canCallApi),
    isMock: !canCallApi,
  });

  const fetchRecords = async (signal?: AbortSignal) => {
    if (!baseUrl || !canCallApi) {
      setState({ data: fallback, loading: false, isMock: true });
      return;
    }

    setState((prev) => ({ ...prev, loading: true, error: undefined }));
    try {
      const resp = await fetch(`${baseUrl}/api/v1/classifications/pending`, {
        headers,
        signal,
      });
      if (!resp.ok) {
        throw new Error(`request failed (${resp.status})`);
      }
      const body = (await resp.json()) as Array<{
        normalized_key: string;
        status: string;
        base_url?: string;
        last_error?: string;
        requested_at: string;
        updated_at: string;
      }>;
      setState({
        data: body.map((row) => ({
          normalizedKey: row.normalized_key,
          status: row.status,
          baseUrl: row.base_url,
          lastError: row.last_error,
          requestedAt: row.requested_at,
          updatedAt: row.updated_at,
        })),
        loading: false,
        error: undefined,
        isMock: false,
      });
    } catch (err) {
      if (signal?.aborted) return;
      setState({
        data: fallback,
        loading: false,
        error: err instanceof Error ? err.message : 'Failed to fetch pending classifications',
        isMock: true,
      });
    }
  };

  useEffect(() => {
    const controller = new AbortController();
    fetchRecords(controller.signal);
    return () => controller.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [baseUrl, canCallApi]);

  return {
    ...state,
    refresh: () => fetchRecords(),
    canCallApi,
    baseUrl,
    headers,
  } as const;
};
