import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useDashboardLlmSeries } from './useDashboardLlmSeries';

const mockFetch = vi.fn();

const apiStub = {
  baseUrl: 'https://admin.test',
  headers: { Authorization: 'Bearer test' },
  canCallApi: true,
};

vi.mock('./useAdminApi', () => ({
  useAdminApi: () => apiStub,
}));

describe('useDashboardLlmSeries', () => {
  const createWrapper = () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });

    return ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
  };

  beforeEach(() => {
    mockFetch.mockReset();
    global.fetch = mockFetch as unknown as typeof fetch;
  });

  it('loads LLM telemetry time series for selected range', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({
        range: '15m',
        source: 'live',
        step_seconds: 15,
        providers: [
          {
            provider: 'local-lmstudio',
            success: [{ ts_ms: 1_713_000_000_000, value: 12 }],
            failures: [{ ts_ms: 1_713_000_000_000, value: 3 }],
            timeouts: [{ ts_ms: 1_713_000_000_000, value: 1 }],
            non_retryable_400: [{ ts_ms: 1_713_000_000_000, value: 2 }],
          },
        ],
        errors: [],
      }),
      text: async () => '',
    });

    const { result } = renderHook(() => useDashboardLlmSeries('15m'), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.isMock).toBe(false);
    expect(result.current.data?.providers[0]?.provider).toBe('local-lmstudio');
    expect(result.current.data?.providers[0]?.non_retryable_400[0]?.value).toBe(2);
    expect(mockFetch).toHaveBeenCalledOnce();
    const calledUrl = String(mockFetch.mock.calls[0][0]);
    expect(calledUrl).toContain('/api/v1/reporting/ops-llm-series');
    expect(calledUrl).toContain('range=15m');
  });

  it('returns mock mode when endpoint fails', async () => {
    mockFetch.mockRejectedValueOnce(new Error('ops llm down'));

    const { result } = renderHook(() => useDashboardLlmSeries('5m'), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.isMock).toBe(true);
    expect(result.current.error).toMatch(/ops llm down/i);
  });
});
