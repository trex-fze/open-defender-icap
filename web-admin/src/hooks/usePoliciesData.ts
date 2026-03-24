import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import type { PolicyRule } from '../data/mockData';
import { policies } from '../data/mockData';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type PolicyListItem = {
  id: string;
  name: string;
  version: string;
  status: string;
  ruleCount: number;
};

export type PolicyDetail = {
  id: string;
  name: string;
  version: string;
  status: string;
  ruleCount: number;
  rules: PolicyRule[];
};

type PolicyListResponse = {
  data: ApiPolicySummary[];
};

type ApiPolicySummary = {
  id: string;
  name: string;
  version: string;
  status: string;
  rule_count?: number;
};

type ApiPolicyDetail = {
  id: string;
  name: string;
  version: string;
  status: string;
  rule_count?: number;
  rules?: ApiPolicyRule[];
};

type ApiPolicyRule = {
  id: string;
  description?: string;
  priority: number;
  action: string;
  conditions?: Record<string, unknown>;
};

type PoliciesState = {
  data: PolicyListItem[];
  loading: boolean;
  error?: string;
  isMock: boolean;
};

type PolicyDetailState = {
  data?: PolicyDetail;
  loading: boolean;
  error?: string;
  isMock: boolean;
};

const mockSummaries: PolicyListItem[] = policies.map((policy) => ({
  id: policy.id,
  name: policy.name,
  version: policy.version,
  status: policy.status,
  ruleCount: policy.rules.length,
}));

const mockDetail = (policyId: string | undefined): PolicyDetail | undefined => {
  if (!policyId) return undefined;
  const match = policies.find((p) => p.id === policyId);
  if (!match) return undefined;
  return {
    id: match.id,
    name: match.name,
    version: match.version,
    status: match.status,
    ruleCount: match.rules.length,
    rules: match.rules,
  };
};

export const usePoliciesData = (): PoliciesState => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.policies(baseUrl),
    enabled,
    queryFn: async () => {
      const body = await adminGetJson<PolicyListResponse>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/policies',
        { include_drafts: true },
      );
      return (body.data ?? []).map(mapSummary);
    },
  });

  if (!enabled) {
    return { data: mockSummaries, loading: false, isMock: true };
  }

  if (query.isError) {
    return {
      data: mockSummaries,
      loading: false,
      error: query.error instanceof Error ? query.error.message : 'Failed to reach Admin API',
      isMock: true,
    };
  }

  return {
    data: query.data ?? mockSummaries,
    loading: query.isLoading,
    error: undefined,
    isMock: false,
  };
};

export const usePolicyDetail = (policyId?: string): PolicyDetailState => {
  const fallback = useMemo(() => mockDetail(policyId), [policyId]);
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi && policyId);

  const query = useQuery({
    queryKey: queryKeys.policyDetail(baseUrl, policyId),
    enabled,
    queryFn: async () => {
      const body = await adminGetJson<ApiPolicyDetail>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        `/api/v1/policies/${policyId}`,
      );
      return mapDetail(body);
    },
  });

  if (!policyId) {
    return {
      data: undefined,
      loading: false,
      error: 'Missing policy id',
      isMock: true,
    };
  }

  if (!enabled) {
    return { data: fallback, loading: false, isMock: true };
  }

  if (query.isError) {
    return {
      data: fallback,
      loading: false,
      error: query.error instanceof Error ? query.error.message : 'Failed to reach Admin API',
      isMock: true,
    };
  }

  return {
    data: query.data ?? fallback,
    loading: query.isLoading,
    error: undefined,
    isMock: false,
  };
};

const mapSummary = (item: ApiPolicySummary): PolicyListItem => ({
  id: item.id,
  name: item.name,
  version: item.version,
  status: item.status,
  ruleCount: item.rule_count ?? 0,
});

const mapDetail = (item: ApiPolicyDetail): PolicyDetail => ({
  id: item.id,
  name: item.name,
  version: item.version,
  status: item.status,
  ruleCount: item.rule_count ?? (item.rules?.length ?? 0),
  rules: (item.rules ?? []).map(mapRule),
});

const mapRule = (rule: ApiPolicyRule): PolicyRule => ({
  id: rule.id,
  description: rule.description,
  priority: rule.priority,
  action: rule.action,
  conditions: rule.conditions ?? {},
});
