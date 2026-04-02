import { useState } from 'react';
import { adminDelete, adminPatchJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

type UpdatePayload = {
  primary_category: string;
  subcategory: string;
  reason?: string;
};

export const useClassificationActions = () => {
  const api = useAdminApi();
  const [busyKey, setBusyKey] = useState<string | undefined>();
  const [error, setError] = useState<string | undefined>();

  const updateClassification = async (normalizedKey: string, payload: UpdatePayload) => {
    setBusyKey(normalizedKey);
    setError(undefined);
    try {
      await adminPatchJson<unknown, UpdatePayload>(
        api as AdminApiContext,
        `/api/v1/classifications/${encodeURIComponent(normalizedKey)}`,
        payload,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update classification');
      throw err;
    } finally {
      setBusyKey(undefined);
    }
  };

  const deleteClassification = async (normalizedKey: string) => {
    setBusyKey(normalizedKey);
    setError(undefined);
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/classifications/${encodeURIComponent(normalizedKey)}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete classification');
      throw err;
    } finally {
      setBusyKey(undefined);
    }
  };

  return {
    updateClassification,
    deleteClassification,
    busyKey,
    error,
    canCallApi: api.canCallApi,
  } as const;
};
