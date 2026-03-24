import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useTrafficReportData } from './useTrafficReportData';

const mockFetch = vi.fn();

const apiStub = {
  baseUrl: 'https://admin.test',
  headers: { Authorization: 'Bearer test' },
  canCallApi: true,
};

vi.mock('./useAdminApi', () => ({
  useAdminApi: () => apiStub,
}));

describe('useTrafficReportData', () => {
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

  it('loads traffic summary from reporting endpoint', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({
        range: '24h',
        bucket_interval: '1h',
        allow_block_trend: [{ action: 'allow', buckets: [{ key_as_string: '2026-03-25T00:00:00Z', doc_count: 3 }] }],
        top_blocked_domains: [{ key: 'bad.example', doc_count: 2 }],
        top_categories: [{ key: 'malware', doc_count: 4 }],
      }),
    });

    const { result } = renderHook(() => useTrafficReportData('24h', 10), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.isMock).toBe(false);
    expect(result.current.data?.top_categories[0]?.key).toBe('malware');
    expect(mockFetch).toHaveBeenCalledOnce();
  });

  it('falls back to mock mode when request fails', async () => {
    mockFetch.mockRejectedValueOnce(new Error('traffic api down'));

    const { result } = renderHook(() => useTrafficReportData('6h', 5), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.isMock).toBe(true);
    expect(result.current.error).toMatch(/traffic api down/i);
  });
});
