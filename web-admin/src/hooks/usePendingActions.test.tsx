import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePendingActions } from './usePendingActions';

const mockFetch = vi.fn();

const apiStub = {
  baseUrl: 'https://admin.test',
  headers: { Authorization: 'Bearer test' },
  canCallApi: true,
};

vi.mock('./useAdminApi', () => ({
  useAdminApi: () => apiStub,
}));

describe('usePendingActions', () => {
  beforeEach(() => {
    mockFetch.mockReset();
    global.fetch = mockFetch as unknown as typeof fetch;
  });

  it('posts manual decision to unblock endpoint', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({}),
    });

    const { result } = renderHook(() => usePendingActions());

    await act(async () => {
      await result.current.manualUnblock('domain:test.example', {
        action: 'Allow',
        primary_category: 'Manual Override',
        subcategory: 'Analyst Override',
        risk_level: 'low',
        confidence: 0.95,
        reason: 'validated by analyst',
      });
    });

    expect(mockFetch).toHaveBeenCalledOnce();
    expect(mockFetch.mock.calls[0]?.[0]).toContain('/api/v1/classifications/domain%3Atest.example/unblock');
    expect(result.current.error).toBeUndefined();
  });

  it('stores error when request fails', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 500,
      json: async () => ({ message: 'db unavailable' }),
    });

    const { result } = renderHook(() => usePendingActions());

    await expect(
      act(async () => {
        await result.current.manualUnblock('domain:test.example', {
          action: 'Block',
          primary_category: 'Manual Override',
          subcategory: 'Analyst Override',
          risk_level: 'high',
          confidence: 0.9,
          reason: 'malicious payload',
        });
      }),
    ).rejects.toThrow(/db unavailable/i);

    expect(result.current.busyKey).toBeUndefined();
  });
});
