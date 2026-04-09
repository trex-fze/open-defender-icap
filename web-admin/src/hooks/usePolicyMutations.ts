import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { adminDelete, adminPostJson, adminPutJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from './useAdminApi';
import { queryKeys } from './queryKeys';

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
  version?: string;
  notes?: string;
};

type PolicyUpdateRequest = {
  name?: string;
  version?: string;
  status?: string;
  notes?: string;
  rules?: PolicyRulePayload[];
};

type PolicyValidationResponse = {
  valid: boolean;
  errors: string[];
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
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();

  const invalidatePolicyQueries = async (policyId?: string) => {
    await queryClient.invalidateQueries({
      predicate: (query) => Array.isArray(query.queryKey) && query.queryKey[0] === 'policies',
    });
    if (policyId) {
      await queryClient.invalidateQueries({ queryKey: queryKeys.policyDetail(api.baseUrl, policyId) });
      await queryClient.invalidateQueries({ queryKey: queryKeys.policyVersions(api.baseUrl, policyId) });
    }
  };

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
      await invalidatePolicyQueries(result.id);
      return result.id;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create policy draft';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const publishPolicy = async (policyId: string, notes?: string, version?: string): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const payload: PolicyPublishRequest = {
        version: version?.trim() || undefined,
        notes: notes?.trim() || undefined,
      };
      await adminPostJson<unknown, PolicyPublishRequest>(
        api as AdminApiContext,
        `/api/v1/policies/${policyId}/publish`,
        payload,
      );
      await invalidatePolicyQueries(policyId);
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
    input: { name?: string; version?: string; status?: string; notes?: string; rules?: PolicyRulePayload[] },
  ): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const payload: PolicyUpdateRequest = {
        name: input.name?.trim() || undefined,
        version: input.version?.trim() || undefined,
        status: input.status?.trim().toLowerCase() || undefined,
        notes: input.notes?.trim() || undefined,
        rules: input.rules,
      };
      await adminPutJson<unknown, PolicyUpdateRequest>(
        api as AdminApiContext,
        `/api/v1/policies/${policyId}`,
        payload,
      );
      await invalidatePolicyQueries(policyId);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to update policy';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const validatePolicy = async (input: {
    name: string;
    version?: string;
    notes?: string;
    rules: PolicyRulePayload[];
  }): Promise<PolicyValidationResponse> => {
    setBusy(true);
    setError(undefined);
    try {
      return await adminPostJson<PolicyValidationResponse, PolicyDraftRequest>(
        api as AdminApiContext,
        '/api/v1/policies/validate',
        {
          name: input.name.trim(),
          version: input.version?.trim() || undefined,
          notes: input.notes?.trim() || undefined,
          rules: input.rules,
        },
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to validate policy rules';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const disablePolicy = async (policyId: string, notes?: string): Promise<void> => {
    await updatePolicy(policyId, {
      status: 'archived',
      notes: notes?.trim() || 'Archived via web-admin',
    });
  };

  const deletePolicy = async (policyId: string): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/policies/${policyId}`);
      await invalidatePolicyQueries(policyId);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to delete policy';
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
    validatePolicy,
    disablePolicy,
    deletePolicy,
    busy,
    error,
    canCallApi: api.canCallApi,
  };
};
