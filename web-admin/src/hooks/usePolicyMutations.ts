import { useState } from 'react';
import { adminPostJson, adminPutJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

export type PolicyCreateInput = {
  name: string;
  version?: string;
  notes?: string;
};

type PolicyRulePayload = {
  id: string;
  description: string;
  priority: number;
  action: string;
  conditions: Record<string, unknown>;
};

type PolicyDraftRequest = {
  name: string;
  version?: string;
  notes?: string;
  rules: PolicyRulePayload[];
};

type PolicyDetailResponse = {
  id: string;
};

type PolicyPublishRequest = {
  notes?: string;
};

type PolicyUpdateRequest = {
  version?: string;
  notes?: string;
  rules?: PolicyRulePayload[];
};

const defaultStarterRule = (): PolicyRulePayload => ({
  id: 'starter-monitor',
  description: 'Starter monitor rule. Replace with your policy conditions.',
  priority: 100,
  action: 'Monitor',
  conditions: {},
});

export const usePolicyMutations = () => {
  const api = useAdminApi();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();

  const createDraft = async (input: PolicyCreateInput): Promise<string> => {
    setBusy(true);
    setError(undefined);
    try {
      const payload: PolicyDraftRequest = {
        name: input.name.trim(),
        version: input.version?.trim() || undefined,
        notes: input.notes?.trim() || undefined,
        rules: [defaultStarterRule()],
      };
      const result = await adminPostJson<PolicyDetailResponse, PolicyDraftRequest>(
        api as AdminApiContext,
        '/api/v1/policies',
        payload,
      );
      return result.id;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create policy draft';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const publishPolicy = async (policyId: string, notes?: string): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const payload: PolicyPublishRequest = {
        notes: notes?.trim() || undefined,
      };
      await adminPostJson<unknown, PolicyPublishRequest>(
        api as AdminApiContext,
        `/api/v1/policies/${policyId}/publish`,
        payload,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to publish policy';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const updatePolicy = async (
    policyId: string,
    input: { version?: string; notes?: string; rules: PolicyRulePayload[] },
  ): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const payload: PolicyUpdateRequest = {
        version: input.version?.trim() || undefined,
        notes: input.notes?.trim() || undefined,
        rules: input.rules,
      };
      await adminPutJson<unknown, PolicyUpdateRequest>(
        api as AdminApiContext,
        `/api/v1/policies/${policyId}`,
        payload,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to update policy';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  return {
    createDraft,
    publishPolicy,
    updatePolicy,
    busy,
    error,
    canCallApi: api.canCallApi,
  };
};
