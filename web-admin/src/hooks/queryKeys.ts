export const queryKeys = {
  policies: (baseUrl: string) => ['policies', baseUrl] as const,
  policyDetail: (baseUrl: string, policyId?: string) => ['policy-detail', baseUrl, policyId ?? 'missing'] as const,
  overrides: (baseUrl: string) => ['overrides', baseUrl] as const,
  taxonomy: (baseUrl: string) => ['taxonomy', baseUrl] as const,
  pendingClassifications: (baseUrl: string) => ['pending-classifications', baseUrl] as const,
  reportingAggregates: (baseUrl: string, dimension: string) => ['reporting-aggregates', baseUrl, dimension] as const,
  reportingTraffic: (baseUrl: string, range: string, topN: number, bucket?: string) =>
    ['reporting-traffic', baseUrl, range, topN, bucket ?? 'auto'] as const,
};
