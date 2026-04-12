import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

type ProviderSummary = {
  name: string;
  provider_type?: string;
  role?: string;
  health_status?: 'healthy' | 'degraded' | 'unreachable' | 'misconfigured' | 'unknown';
};

export type LlmProviderOption = {
  name: string;
  providerType: string;
  role: string;
  healthStatus: 'healthy' | 'degraded' | 'unreachable' | 'misconfigured' | 'unknown';
};

const mapProviders = (providers: ProviderSummary[]): LlmProviderOption[] =>
  providers.map((item) => ({
    name: item.name,
    providerType: item.provider_type ?? 'unknown',
    role: item.role ?? 'unknown',
    healthStatus: item.health_status ?? 'unknown',
  }));

export const useLlmProviders = () => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.llmProviders(baseUrl),
    enabled,
    queryFn: async (): Promise<LlmProviderOption[]> => {
      const providers = await adminGetJson<ProviderSummary[]>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/ops/llm/providers',
      );
      return mapProviders(providers);
    },
  });

  return {
    data: query.data ?? [],
    loading: enabled ? query.isLoading : false,
    error: query.isError
      ? query.error instanceof Error
        ? query.error.message
        : 'Failed to load LLM providers'
      : undefined,
    canCallApi,
  } as const;
};
