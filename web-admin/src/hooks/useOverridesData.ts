import { useEffect, useState } from 'react';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { overrides } from '../data/mockData';
import { useAdminApi } from './useAdminApi';

export type OverrideRow = {
  id: string;
  scopeType: string;
  scopeValue: string;
  scope: string;
  action: string;
  status: string;
  reason?: string;
  expiresAt?: string;
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
  reason?: string;
  expires_at?: string;
};

const fallbackRows: OverrideRow[] = overrides.map((item) => ({
  id: item.id,
  scopeType: item.scope.split(':')[0] ?? 'domain',
  scopeValue: item.scope.split(':').slice(1).join(':') || item.scope,
  scope: item.scope,
  action: item.action,
  status: item.status,
  reason: undefined,
  expiresAt: undefined,
  expires: item.expires,
}));

const mapOverride = (record: ApiOverrideRecord): OverrideRow => ({
  id: record.id,
  scopeType: record.scope_type,
  scopeValue: record.scope_value,
  scope: `${record.scope_type}:${record.scope_value}`,
  action: record.action ?? 'unknown',
  status: record.status,
  reason: record.reason,
  expiresAt: record.expires_at,
  expires: record.expires_at
    ? new Date(record.expires_at).toLocaleString()
    : 'never',
});

export const useOverridesData = () => {
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
    const fetchOverrides = async (signal?: AbortSignal) => {
      setState((prev) => ({ ...prev, loading: true, error: undefined }));
      try {
        const body = await adminGetJson<ApiOverrideRecord[]>(
          { baseUrl, canCallApi, headers } as AdminApiContext,
          '/api/v1/overrides',
          undefined,
          { signal },
        );
        if (!cancelled) {
          setState({ data: body.map(mapOverride), loading: false, isMock: false });
        }
      } catch (err) {
        if (signal?.aborted || cancelled) {
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

    fetchOverrides(controller.signal);
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [baseUrl, canCallApi, headers]);

  const refresh = async () => {
    const controller = new AbortController();
    if (!baseUrl || !canCallApi) {
      setState({ data: fallbackRows, loading: false, isMock: true });
      return;
    }

    setState((prev) => ({ ...prev, loading: true, error: undefined }));
    try {
      const body = await adminGetJson<ApiOverrideRecord[]>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/overrides',
        undefined,
        { signal: controller.signal },
      );
      setState({ data: body.map(mapOverride), loading: false, isMock: false });
    } catch (err) {
      if (controller.signal.aborted) return;
      setState({
        data: fallbackRows,
        loading: false,
        error: err instanceof Error ? err.message : 'Failed to fetch overrides',
        isMock: true,
      });
    }
  };

  return {
    ...state,
    refresh,
    canCallApi,
    baseUrl,
    headers,
  } as const;
};
