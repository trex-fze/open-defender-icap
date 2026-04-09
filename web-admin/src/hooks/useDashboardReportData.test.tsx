import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useDashboardReportData } from './useDashboardReportData';

const mockFetch = vi.fn();

const apiStub = {
  baseUrl: 'https://admin.test',
  headers: { Authorization: 'Bearer test' },
  canCallApi: true,
};

vi.mock('./useAdminApi', () => ({
  useAdminApi: () => apiStub,
}));

describe('useDashboardReportData', () => {
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

  it('loads dashboard analytics from reporting endpoint', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({
        range: '24h',
        bucket_interval: '1h',
        overview: {
          total_requests: 200,
          allow_requests: 170,
          blocked_requests: 30,
          block_rate: 0.15,
          unique_clients: 12,
          total_bandwidth_bytes: 123456,
        },
        hourly_usage: [
          {
            timestamp: '2026-04-10T00:00:00.000Z',
            total_requests: 10,
            blocked_requests: 2,
            bandwidth_bytes: 4096,
          },
        ],
        top_domains: [{ key: 'example.com', doc_count: 42 }],
        top_blocked_domains: [{ key: 'blocked.example', doc_count: 5 }],
        top_blocked_requesters: [{ key: '192.168.1.253', doc_count: 3 }],
        top_clients_by_bandwidth: [{ key: '192.168.1.253', doc_count: 30, bandwidth_bytes: 8192 }],
        coverage: {
          total_docs: 200,
          client_ip_docs: 200,
          domain_docs: 190,
          network_bytes_docs: 180,
        },
      }),
      text: async () => '',
    });

    const { result } = renderHook(() => useDashboardReportData('24h', 10), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.isMock).toBe(false);
    expect(result.current.data?.overview.unique_clients).toBe(12);
    expect(result.current.data?.top_blocked_requesters[0]?.key).toBe('192.168.1.253');
    expect(mockFetch).toHaveBeenCalledOnce();
  });

  it('returns mock mode when request fails', async () => {
    mockFetch.mockRejectedValueOnce(new Error('dashboard api down'));

    const { result } = renderHook(() => useDashboardReportData('6h', 5), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.isMock).toBe(true);
    expect(result.current.error).toMatch(/dashboard api down/i);
  });

  it('maps 401 to session-expired message', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 401,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({ message: 'unauthorized' }),
      text: async () => JSON.stringify({ message: 'unauthorized' }),
    });

    const { result } = renderHook(() => useDashboardReportData('24h', 10), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.isMock).toBe(true);
    expect(result.current.error).toBe('Session expired. Please sign in again.');
  });
});
