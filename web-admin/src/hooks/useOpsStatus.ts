import { useEffect, useState } from 'react';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

type PendingRecord = {
  normalized_key: string;
};

type ProviderSummary = {
  name: string;
  provider_type: string;
  endpoint: string;
  role: string;
};

export type OpsSnapshot = {
  pendingCount: number;
  llmProviderNames: string[];
  source: 'live' | 'partial' | 'mock';
};

const PROVIDERS_URL = (import.meta.env.VITE_LLM_PROVIDERS_URL ?? '').trim();

export const useOpsStatus = () => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const [data, setData] = useState<OpsSnapshot>({
    pendingCount: 0,
    llmProviderNames: [],
    source: 'mock',
  });
  const [loading, setLoading] = useState(Boolean(canCallApi));
  const [error, setError] = useState<string | undefined>();

  useEffect(() => {
    if (!baseUrl || !canCallApi) {
      setData({ pendingCount: 0, llmProviderNames: [], source: 'mock' });
      setLoading(false);
      return;
    }

    let cancelled = false;
    const controller = new AbortController();

    const fetchOps = async () => {
      setLoading(true);
      setError(undefined);
      try {
        const pending = await adminGetJson<PendingRecord[]>(
          { baseUrl, canCallApi, headers } as AdminApiContext,
          '/api/v1/classifications/pending',
          { limit: 500 },
          { signal: controller.signal },
        );

        let providers: ProviderSummary[] = [];
        let source: OpsSnapshot['source'] = 'live';
        if (PROVIDERS_URL) {
          try {
            const resp = await fetch(PROVIDERS_URL, { signal: controller.signal });
            if (resp.ok) {
              providers = (await resp.json()) as ProviderSummary[];
            } else {
              source = 'partial';
            }
          } catch {
            source = 'partial';
          }
        } else {
          source = 'partial';
        }

        if (!cancelled) {
          setData({
            pendingCount: pending.length,
            llmProviderNames: providers.map((item) => item.name),
            source,
          });
        }
      } catch (err) {
        if (controller.signal.aborted || cancelled) return;
        setError(err instanceof Error ? err.message : 'Failed to fetch operations status');
        setData({ pendingCount: 0, llmProviderNames: [], source: 'mock' });
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    };

    fetchOps();
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [baseUrl, canCallApi, headers]);

  return { data, loading, error } as const;
};
