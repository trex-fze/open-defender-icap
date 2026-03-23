import { useEffect, useMemo, useState } from 'react';
import type { PolicySummary, PolicyRule } from '../data/mockData';
import { policies } from '../data/mockData';

const API_BASE_URL = (import.meta.env.VITE_ADMIN_API_URL ?? '').trim();
const ADMIN_TOKEN = (import.meta.env.VITE_ADMIN_TOKEN ?? '').trim();

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
  const [state, setState] = useState<PoliciesState>({
    data: mockSummaries,
    loading: Boolean(API_BASE_URL),
    isMock: !API_BASE_URL,
  });

  useEffect(() => {
    if (!API_BASE_URL) {
      setState({ data: mockSummaries, loading: false, isMock: true });
      return;
    }

    let cancelled = false;
    const controller = new AbortController();
    setState((prev) => ({ ...prev, loading: true, error: undefined }));

    const run = async () => {
      try {
        const url = new URL('/api/v1/policies', API_BASE_URL);
        url.searchParams.set('include_drafts', 'true');
        const resp = await fetch(url, {
          headers: buildHeaders(),
          signal: controller.signal,
        });
        if (!resp.ok) {
          throw new Error(`Request failed (${resp.status})`);
        }
        const body = (await resp.json()) as PolicyListResponse;
        const next = (body.data ?? []).map(mapSummary);
        if (!cancelled) {
          setState({ data: next, loading: false, isMock: false });
        }
      } catch (err) {
        if (controller.signal.aborted || cancelled) {
          return;
        }
        console.warn('[Policies] falling back to mock data', err);
        setState({
          data: mockSummaries,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to reach Admin API',
          isMock: true,
        });
      }
    };

    run();
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, []);

  return state;
};

export const usePolicyDetail = (policyId?: string): PolicyDetailState => {
  const fallback = useMemo(() => mockDetail(policyId), [policyId]);
  const [state, setState] = useState<PolicyDetailState>({
    data: fallback,
    loading: Boolean(API_BASE_URL && policyId),
    isMock: !API_BASE_URL,
  });

  useEffect(() => {
    if (!policyId) {
      setState({
        data: undefined,
        loading: false,
        error: 'Missing policy id',
        isMock: true,
      });
      return;
    }

    if (!API_BASE_URL) {
      setState({ data: fallback, loading: false, isMock: true });
      return;
    }

    let cancelled = false;
    const controller = new AbortController();
    setState((prev) => ({ ...prev, loading: true, error: undefined }));

    const run = async () => {
      try {
        const url = new URL(`/api/v1/policies/${policyId}`, API_BASE_URL);
        const resp = await fetch(url, {
          headers: buildHeaders(),
          signal: controller.signal,
        });
        if (!resp.ok) {
          throw new Error(`Request failed (${resp.status})`);
        }
        const body = (await resp.json()) as ApiPolicyDetail;
        if (!cancelled) {
          setState({ data: mapDetail(body), loading: false, isMock: false });
        }
      } catch (err) {
        if (controller.signal.aborted || cancelled) {
          return;
        }
        console.warn('[PolicyDetail] falling back to mock data', err);
        setState({
          data: fallback,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to reach Admin API',
          isMock: true,
        });
      }
    };

    run();
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [policyId, fallback]);

  return state;
};

const buildHeaders = () => {
  const headers: HeadersInit = {
    Accept: 'application/json',
  };
  if (ADMIN_TOKEN) {
    headers['X-Admin-Token'] = ADMIN_TOKEN;
  }
  return headers;
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
