import { useState } from 'react';
import { adminDelete, adminDeleteJson, adminPostJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

type ManualClassifyPayload = {
  primary_category: string;
  subcategory: string;
  reason?: string;
};

type ClearAllPendingResponse = {
  deleted: number;
};

export const usePendingActions = () => {
  const api = useAdminApi();
  const [busyKey, setBusyKey] = useState<string | undefined>();
  const [busyAll, setBusyAll] = useState(false);
  const [error, setError] = useState<string | undefined>();

  const manualClassify = async (
    normalizedKey: string,
    payload: ManualClassifyPayload,
  ) => {
    setBusyKey(normalizedKey);
    setError(undefined);
    try {
      await adminPostJson<unknown, ManualClassifyPayload>(
        api as AdminApiContext,
        `/api/v1/classifications/${encodeURIComponent(normalizedKey)}/manual-classify`,
        payload,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update pending classification');
      throw err;
    } finally {
      setBusyKey(undefined);
    }
  };

  const clearPending = async (normalizedKey: string) => {
    setBusyKey(normalizedKey);
    setError(undefined);
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/classifications/${encodeURIComponent(normalizedKey)}/pending`);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete pending site');
      throw err;
    } finally {
      setBusyKey(undefined);
    }
  };

  const clearAllPending = async (): Promise<number> => {
    setBusyAll(true);
    setError(undefined);
    try {
      const response = await adminDeleteJson<ClearAllPendingResponse>(
        api as AdminApiContext,
        '/api/v1/classifications/pending',
      );
      return response.deleted;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete all pending sites');
      throw err;
    } finally {
      setBusyAll(false);
    }
  };

  return {
    manualClassify,
    clearPending,
    clearAllPending,
    busyKey,
    busyAll,
    error,
    canCallApi: api.canCallApi,
  } as const;
};
