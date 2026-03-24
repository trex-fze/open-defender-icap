import { useState } from 'react';
import { adminPostJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

type ManualUnblockPayload = {
  action: string;
  primary_category: string;
  subcategory: string;
  risk_level: string;
  confidence: number;
  reason?: string;
};

export const usePendingActions = () => {
  const api = useAdminApi();
  const [busyKey, setBusyKey] = useState<string | undefined>();
  const [error, setError] = useState<string | undefined>();

  const manualUnblock = async (
    normalizedKey: string,
    payload: ManualUnblockPayload,
  ) => {
    setBusyKey(normalizedKey);
    setError(undefined);
    try {
      await adminPostJson<unknown, ManualUnblockPayload>(
        api as AdminApiContext,
        `/api/v1/classifications/${encodeURIComponent(normalizedKey)}/unblock`,
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
    manualUnblock,
    busyKey,
    error,
    canCallApi: api.canCallApi,
  } as const;
};
