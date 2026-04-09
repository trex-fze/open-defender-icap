import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { adminGetJson, adminPostJson, adminPutJson, type AdminApiContext } from './adminClient';

const baseCtx: AdminApiContext = {
  baseUrl: 'http://admin-api.local',
  canCallApi: true,
  headers: {},
};

describe('adminClient JSON helpers', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('adminPostJson sets application/json content type by default', async () => {
    const mockResponse = {
      ok: true,
      status: 200,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({ ok: true }),
      text: async () => JSON.stringify({ ok: true }),
    } as Response;
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(mockResponse);

    await adminPostJson(baseCtx, '/api/test', { foo: 'bar' });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [, init] = fetchMock.mock.calls[0];
    const headers = new Headers(init?.headers);
    expect(headers.get('Content-Type')).toBe('application/json');
    expect(init?.body).toBe(JSON.stringify({ foo: 'bar' }));
  });

  it('adminPutJson respects custom content type when provided', async () => {
    const mockResponse = {
      ok: true,
      status: 200,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({ ok: true }),
      text: async () => JSON.stringify({ ok: true }),
    } as Response;
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(mockResponse);

    await adminPutJson(
      baseCtx,
      '/api/test',
      { foo: 'bar' },
      { headers: { 'Content-Type': 'application/merge-patch+json' } },
    );

    const [, init] = fetchMock.mock.calls[0];
    const headers = new Headers(init?.headers);
    expect(headers.get('Content-Type')).toBe('application/merge-patch+json');
  });

  it('adminPutJson sets application/json when caller omits it', async () => {
    const mockResponse = {
      ok: true,
      status: 200,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({ ok: true }),
      text: async () => JSON.stringify({ ok: true }),
    } as Response;
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(mockResponse);

    await adminPutJson(baseCtx, '/api/test', { foo: 'bar' });

    const [, init] = fetchMock.mock.calls[0];
    const headers = new Headers(init?.headers);
    expect(headers.get('Content-Type')).toBe('application/json');
    expect(init?.body).toBe(JSON.stringify({ foo: 'bar' }));
  });

  it('calls onUnauthorized callback for 401 responses', async () => {
    const onUnauthorized = vi.fn();
    const ctx: AdminApiContext = {
      ...baseCtx,
      onUnauthorized,
    };
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 401,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({ message: 'unauthorized' }),
    } as Response);

    await expect(adminGetJson(ctx, '/api/test')).rejects.toThrow(/unauthorized/i);
    expect(onUnauthorized).toHaveBeenCalledWith(401);
  });
});
