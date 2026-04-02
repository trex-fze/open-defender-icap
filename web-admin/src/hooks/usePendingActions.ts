import { useState } from 'react';
import { adminPostJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

type ManualClassifyPayload = {
  primary_category: string;
  subcategory: string;
  reason?: string;
};

export const usePendingActions = () => {
  const api = useAdminApi();
  const [busyKey, setBusyKey] = useState<string | undefined>();
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

  return {
    manualClassify,
    busyKey,
    error,
    canCallApi: api.canCallApi,
  } as const;
};
