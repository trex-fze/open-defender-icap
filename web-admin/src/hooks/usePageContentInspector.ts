import { useState } from 'react';
import { AdminApiError, adminGetJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

export type PageContentRecord = {
  normalized_key: string;
  fetch_version: number;
  content_type?: string;
  content_hash?: string;
  char_count?: number;
  byte_count?: number;
  fetch_status: string;
  fetch_reason?: string;
  ttl_seconds: number;
  fetched_at: string;
  expires_at: string;
  excerpt?: string;
  excerpt_truncated: boolean;
  excerpt_format?: string;
  source_url?: string;
  resolved_url?: string;
  attempt_summary?: string;
};

export type PageContentSummary = {
  fetch_version: number;
  fetch_status: string;
  fetch_reason?: string;
  ttl_seconds: number;
  fetched_at: string;
  expires_at: string;
  char_count?: number;
  byte_count?: number;
  content_hash?: string;
  resolved_url?: string;
};

export const usePageContentInspector = () => {
  const api = useAdminApi();
  const [record, setRecord] = useState<PageContentRecord | undefined>();
  const [history, setHistory] = useState<PageContentSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | undefined>();

  const lookup = async (key: string, excerpt = 1200) => {
    setLoading(true);
    setError(undefined);
    setRecord(undefined);
    setHistory([]);
    try {
      const encoded = encodeURIComponent(key.trim());
      const [latest, versions] = await Promise.all([
        adminGetJson<PageContentRecord>(
          api as AdminApiContext,
          `/api/v1/page-contents/${encoded}`,
          { max_excerpt: excerpt },
        ),
        adminGetJson<PageContentSummary[]>(
          api as AdminApiContext,
          `/api/v1/page-contents/${encoded}/history`,
          { limit: 10 },
        ),
      ]);
      setRecord(latest);
      setHistory(versions);
    } catch (err) {
      if (err instanceof AdminApiError && err.status === 404) {
        setError('No fetched markdown content exists yet for this key. Trigger traffic first and retry.');
      } else {
        setError(err instanceof Error ? err.message : 'Failed to fetch page content');
      }
    } finally {
      setLoading(false);
    }
  };

  return {
    lookup,
    record,
    history,
    loading,
    error,
    canCallApi: api.canCallApi,
  } as const;
};
