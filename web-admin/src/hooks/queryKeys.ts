export const queryKeys = {
  policies: (
    baseUrl: string,
    cursor: string,
    limit: number,
    status: string,
    search: string,
    includeDrafts: boolean,
  ) => ['policies', baseUrl, cursor, limit, status, search, includeDrafts] as const,
  policyDetail: (baseUrl: string, policyId?: string) => ['policy-detail', baseUrl, policyId ?? 'missing'] as const,
  policyVersions: (baseUrl: string, policyId?: string) =>
    ['policy-versions', baseUrl, policyId ?? 'missing'] as const,
  overrides: (baseUrl: string, cursor: string, limit: number) =>
    ['overrides', baseUrl, cursor, limit] as const,
  taxonomy: (baseUrl: string) => ['taxonomy', baseUrl] as const,
  pendingClassifications: (baseUrl: string, status: string, cursor: string, limit: number) =>
    ['pending-classifications', baseUrl, status, cursor, limit] as const,
  classifications: (
    baseUrl: string,
    state: string,
    q: string,
    cursor: string,
    limit: number,
  ) => ['classifications', baseUrl, state, q, cursor, limit] as const,
  reportingDashboard: (baseUrl: string, range: string, topN: number, bucket?: string) =>
    ['reporting-dashboard', baseUrl, range, topN, bucket ?? 'auto'] as const,
  reportingOpsSummary: (baseUrl: string, range: string) => ['reporting-ops-summary', baseUrl, range] as const,
  reportingOpsLlmSeries: (baseUrl: string, range: string) => ['reporting-ops-llm-series', baseUrl, range] as const,
  opsStatus: (baseUrl: string, providersUrl: string) => ['ops-status', baseUrl, providersUrl] as const,
  platformHealth: (baseUrl: string) => ['platform-health', baseUrl] as const,
  llmProviders: (baseUrl: string) => ['llm-providers', baseUrl] as const,
};
