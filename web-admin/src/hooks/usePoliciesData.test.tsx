import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { usePoliciesData } from './usePoliciesData';

const mockFetch = vi.fn();
const apiStub = {
  baseUrl: 'https://admin.test',
  headers: { Authorization: 'Bearer test' },
  canCallApi: true
};

vi.mock('./useAdminApi', () => ({
  useAdminApi: () => apiStub
}));

describe('usePoliciesData', () => {
  beforeEach(() => {
    mockFetch.mockReset();
    global.fetch = mockFetch as unknown as typeof fetch;
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('returns API data when call succeeds', async () => {
    mockFetch.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        data: [
          {
            id: '123',
            name: 'Live Policy',
            version: 'v42',
            status: 'active',
            rule_count: 5
          }
        ]
      })
    });

    const { result } = renderHook(() => usePoliciesData());

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.isMock).toBe(false);
    expect(result.current.data[0]?.name).toBe('Live Policy');
    expect(mockFetch).toHaveBeenCalledOnce();
  });

  it('falls back to mock data when fetch fails', async () => {
    mockFetch.mockRejectedValueOnce(new Error('network down'));
    const { result } = renderHook(() => usePoliciesData());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.isMock).toBe(true);
    expect(result.current.error).toMatch(/network down/i);
  });
});
