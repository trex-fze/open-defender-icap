import { useState } from 'react';
import {
  adminDelete,
  adminPostJson,
  adminPutJson,
  type AdminApiContext,
} from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

export type OverrideFormInput = {
  scopeType: string;
  scopeValue: string;
  action: string;
  status?: string;
  reason?: string;
  expiresAt?: string;
};

type OverridePayload = {
  scope_type: string;
  scope_value: string;
  action: string;
  reason?: string;
  expires_at?: string;
  status?: string;
};

const toPayload = (input: OverrideFormInput): OverridePayload => ({
  scope_type: input.scopeType.trim().toLowerCase(),
  scope_value: input.scopeValue.trim(),
  action: input.action.trim().toLowerCase(),
  reason: input.reason?.trim() || undefined,
  expires_at: input.expiresAt?.trim() || undefined,
  status: input.status?.trim().toLowerCase() || undefined,
});

export const useOverrideActions = () => {
  const api = useAdminApi();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();

  const createOverride = async (input: OverrideFormInput): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      await adminPostJson<unknown, OverridePayload>(
        api as AdminApiContext,
        '/api/v1/overrides',
        toPayload(input),
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create override';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const updateOverride = async (id: string, input: OverrideFormInput): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      await adminPutJson<unknown, OverridePayload>(
        api as AdminApiContext,
        `/api/v1/overrides/${id}`,
        toPayload(input),
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to update override';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const deleteOverride = async (id: string): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/overrides/${id}`);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to delete override';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  return {
    createOverride,
    updateOverride,
    deleteOverride,
    busy,
    error,
    canCallApi: api.canCallApi,
  };
};
