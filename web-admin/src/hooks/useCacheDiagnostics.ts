import { useState } from 'react';
import {
  adminDelete,
  adminGetJson,
  type AdminApiContext,
} from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

export type CacheEntryRecord = {
  cache_key: string;
  value: Record<string, unknown>;
  expires_at: string;
  source?: string;
  created_at: string;
};

export const useCacheDiagnostics = () => {
  const api = useAdminApi();
  const [entry, setEntry] = useState<CacheEntryRecord | undefined>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | undefined>();
  const [message, setMessage] = useState<string | undefined>();

  const lookup = async (key: string) => {
    setLoading(true);
    setError(undefined);
    setMessage(undefined);
    try {
      const encoded = encodeURIComponent(key.trim());
      const row = await adminGetJson<CacheEntryRecord>(
        api as AdminApiContext,
        `/api/v1/cache-entries/${encoded}`,
      );
      setEntry(row);
    } catch (err) {
      setEntry(undefined);
      setError(err instanceof Error ? err.message : 'Failed to fetch cache entry');
    } finally {
      setLoading(false);
    }
  };

  const evict = async (key: string) => {
    setLoading(true);
    setError(undefined);
    setMessage(undefined);
    try {
      const encoded = encodeURIComponent(key.trim());
      await adminDelete(api as AdminApiContext, `/api/v1/cache-entries/${encoded}`);
      setEntry(undefined);
      setMessage(`Deleted cache entry ${key}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete cache entry');
    } finally {
      setLoading(false);
    }
  };

  return {
    lookup,
    evict,
    entry,
    loading,
    error,
    message,
    canCallApi: api.canCallApi,
  } as const;
};
