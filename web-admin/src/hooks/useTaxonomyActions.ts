import { useState } from 'react';
import { adminPutJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

export type ActivationUpdatePayload = {
  version: string;
  categories: {
    id: string;
    enabled: boolean;
    subcategories: {
      id: string;
      enabled: boolean;
    }[];
  }[];
};

export const useTaxonomyActions = () => {
  const api = useAdminApi();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();

  const saveActivation = async (payload: ActivationUpdatePayload): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      await adminPutJson<unknown, ActivationUpdatePayload>(
        api as AdminApiContext,
        '/api/v1/taxonomy/activation',
        payload,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to save taxonomy activation';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  return {
    saveActivation,
    busy,
    error,
    canCallApi: api.canCallApi,
  } as const;
};
