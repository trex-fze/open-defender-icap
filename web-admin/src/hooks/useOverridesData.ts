import { useEffect, useState } from 'react';
import { overrides } from '../data/mockData';
import { useAdminApi } from './useAdminApi';

export type OverrideRow = {
  id: string;
  scope: string;
  action: string;
  status: string;
  expires: string;
};

type OverrideState = {
  data: OverrideRow[];
  loading: boolean;
  error?: string;
  isMock: boolean;
};

type ApiOverrideRecord = {
  id: string;
  scope_type: string;
  scope_value: string;
  action: string;
  status: string;
  expires_at?: string;
};

const fallbackRows: OverrideRow[] = overrides.map((item) => ({
  id: item.id,
  scope: item.scope,
  action: item.action,
  status: item.status,
  expires: item.expires,
}));

const mapOverride = (record: ApiOverrideRecord): OverrideRow => ({
  id: record.id,
  scope: `${record.scope_type}:${record.scope_value}`,
  action: record.action ?? 'unknown',
  status: record.status,
  expires: record.expires_at
    ? new Date(record.expires_at).toLocaleString()
    : 'never',
});

export const useOverridesData = (): OverrideState => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const [state, setState] = useState<OverrideState>({
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
    const fetchOverrides = async () => {
      setState((prev) => ({ ...prev, loading: true, error: undefined }));
      try {
        const resp = await fetch(`${baseUrl}/api/v1/overrides`, {
          headers,
          signal: controller.signal,
        });
        if (!resp.ok) {
          throw new Error(`Request failed (${resp.status})`);
        }
        const body = (await resp.json()) as ApiOverrideRecord[];
        if (!cancelled) {
          setState({ data: body.map(mapOverride), loading: false, isMock: false });
        }
      } catch (err) {
        if (controller.signal.aborted || cancelled) {
          return;
        }
        setState({
          data: fallbackRows,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to fetch overrides',
          isMock: true,
        });
      }
    };

    fetchOverrides();
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [baseUrl, canCallApi, headers]);

  return state;
};
