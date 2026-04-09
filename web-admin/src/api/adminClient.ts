export type AdminApiContext = {
  baseUrl: string;
  canCallApi: boolean;
  headers: HeadersInit;
  onUnauthorized?: (status: number) => void;
};

type QueryParams = Record<string, string | number | boolean | undefined | null>;

export class AdminApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

const buildUrl = (baseUrl: string, path: string, params?: QueryParams): string => {
  const url = new URL(path, baseUrl);
  if (params) {
    Object.entries(params).forEach(([key, value]) => {
      if (value === undefined || value === null) return;
      url.searchParams.set(key, String(value));
    });
  }
  return url.toString();
};

const withHeaders = (base: HeadersInit, init?: RequestInit): Headers => {
  const headers = new Headers(base);
  if (init?.headers) {
    new Headers(init.headers).forEach((value, key) => headers.set(key, value));
  }
  if (init?.body && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  return headers;
};

const ensureCallable = (ctx: AdminApiContext) => {
  if (!ctx.baseUrl || !ctx.canCallApi) {
    throw new AdminApiError(0, 'Admin API is not configured or auth token is missing');
  }
};

const parseErrorBody = async (resp: Response): Promise<string> => {
  try {
    const body = await resp.json();
    if (typeof body?.message === 'string' && body.message.trim().length > 0) {
      return body.message;
    }
    if (typeof body?.error === 'string' && body.error.trim().length > 0) {
      return body.error;
    }
  } catch {
    // no-op
  }
  return `Request failed (${resp.status})`;
};

const mapNetworkError = (err: unknown): AdminApiError => {
  if (err instanceof AdminApiError) {
    return err;
  }
  if (err instanceof Error && err.message.trim().length > 0) {
    return new AdminApiError(0, `Network error: unable to reach Admin API (${err.message})`);
  }
  return new AdminApiError(0, 'Network error: unable to reach Admin API (check URL/CORS/service health)');
};

const notifyUnauthorized = (ctx: AdminApiContext, status: number) => {
  if (status === 401 || status === 403) {
    try {
      ctx.onUnauthorized?.(status);
    } catch {
      // avoid masking original API failure path
    }
  }
};

export const adminGetJson = async <T>(
  ctx: AdminApiContext,
  path: string,
  params?: QueryParams,
  init?: RequestInit,
): Promise<T> => {
  ensureCallable(ctx);
  let resp: Response;
  try {
    resp = await fetch(buildUrl(ctx.baseUrl, path, params), {
      ...init,
      method: 'GET',
      headers: withHeaders(ctx.headers, init),
    });
  } catch (err) {
    throw mapNetworkError(err);
  }
  if (!resp.ok) {
    notifyUnauthorized(ctx, resp.status);
    throw new AdminApiError(resp.status, await parseErrorBody(resp));
  }
  return (await resp.json()) as T;
};

export const adminPostJson = async <TResponse, TBody = unknown>(
  ctx: AdminApiContext,
  path: string,
  body: TBody,
  init?: RequestInit,
): Promise<TResponse> => {
  ensureCallable(ctx);
  const headers = withHeaders(ctx.headers, init);
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  let resp: Response;
  try {
    resp = await fetch(buildUrl(ctx.baseUrl, path), {
      ...init,
      method: 'POST',
      headers,
      body: JSON.stringify(body),
    });
  } catch (err) {
    throw mapNetworkError(err);
  }
  if (!resp.ok) {
    notifyUnauthorized(ctx, resp.status);
    throw new AdminApiError(resp.status, await parseErrorBody(resp));
  }
  if (resp.status === 204 || resp.status === 205) {
    return undefined as TResponse;
  }
  const contentType = resp.headers.get('Content-Type') ?? '';
  if (!contentType.toLowerCase().includes('application/json')) {
    return undefined as TResponse;
  }
  const text = await resp.text();
  if (!text.trim()) {
    return undefined as TResponse;
  }
  return JSON.parse(text) as TResponse;
};

export const adminPutJson = async <TResponse, TBody = unknown>(
  ctx: AdminApiContext,
  path: string,
  body: TBody,
  init?: RequestInit,
): Promise<TResponse> => {
  ensureCallable(ctx);
  const headers = withHeaders(ctx.headers, init);
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  let resp: Response;
  try {
    resp = await fetch(buildUrl(ctx.baseUrl, path), {
      ...init,
      method: 'PUT',
      headers,
      body: JSON.stringify(body),
    });
  } catch (err) {
    throw mapNetworkError(err);
  }
  if (!resp.ok) {
    notifyUnauthorized(ctx, resp.status);
    throw new AdminApiError(resp.status, await parseErrorBody(resp));
  }
  return (await resp.json()) as TResponse;
};

export const adminDelete = async (
  ctx: AdminApiContext,
  path: string,
  init?: RequestInit,
): Promise<void> => {
  ensureCallable(ctx);
  let resp: Response;
  try {
    resp = await fetch(buildUrl(ctx.baseUrl, path), {
      ...init,
      method: 'DELETE',
      headers: withHeaders(ctx.headers, init),
    });
  } catch (err) {
    throw mapNetworkError(err);
  }
  if (!resp.ok) {
    notifyUnauthorized(ctx, resp.status);
    throw new AdminApiError(resp.status, await parseErrorBody(resp));
  }
};

export const adminDeleteJson = async <TResponse>(
  ctx: AdminApiContext,
  path: string,
  init?: RequestInit,
): Promise<TResponse> => {
  ensureCallable(ctx);
  let resp: Response;
  try {
    resp = await fetch(buildUrl(ctx.baseUrl, path), {
      ...init,
      method: 'DELETE',
      headers: withHeaders(ctx.headers, init),
    });
  } catch (err) {
    throw mapNetworkError(err);
  }
  if (!resp.ok) {
    notifyUnauthorized(ctx, resp.status);
    throw new AdminApiError(resp.status, await parseErrorBody(resp));
  }
  if (resp.status === 204 || resp.status === 205) {
    return undefined as TResponse;
  }
  const contentType = resp.headers.get('Content-Type') ?? '';
  if (!contentType.toLowerCase().includes('application/json')) {
    return undefined as TResponse;
  }
  const text = await resp.text();
  if (!text.trim()) {
    return undefined as TResponse;
  }
  return JSON.parse(text) as TResponse;
};

export const adminPatchJson = async <TResponse, TBody = unknown>(
  ctx: AdminApiContext,
  path: string,
  body: TBody,
  init?: RequestInit,
): Promise<TResponse> => {
  ensureCallable(ctx);
  const headers = withHeaders(ctx.headers, init);
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  let resp: Response;
  try {
    resp = await fetch(buildUrl(ctx.baseUrl, path), {
      ...init,
      method: 'PATCH',
      headers,
      body: JSON.stringify(body),
    });
  } catch (err) {
    throw mapNetworkError(err);
  }
  if (!resp.ok) {
    notifyUnauthorized(ctx, resp.status);
    throw new AdminApiError(resp.status, await parseErrorBody(resp));
  }
  return (await resp.json()) as TResponse;
};
