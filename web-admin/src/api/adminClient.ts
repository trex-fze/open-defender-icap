export type AdminApiContext = {
  baseUrl: string;
  canCallApi: boolean;
  headers: HeadersInit;
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

export const adminGetJson = async <T>(
  ctx: AdminApiContext,
  path: string,
  params?: QueryParams,
  init?: RequestInit,
): Promise<T> => {
  ensureCallable(ctx);
  const resp = await fetch(buildUrl(ctx.baseUrl, path, params), {
    ...init,
    method: 'GET',
    headers: withHeaders(ctx.headers, init),
  });
  if (!resp.ok) {
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
  const resp = await fetch(buildUrl(ctx.baseUrl, path), {
    ...init,
    method: 'POST',
    headers,
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    throw new AdminApiError(resp.status, await parseErrorBody(resp));
  }
  return (await resp.json()) as TResponse;
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
  const resp = await fetch(buildUrl(ctx.baseUrl, path), {
    ...init,
    method: 'PUT',
    headers,
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
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
  const resp = await fetch(buildUrl(ctx.baseUrl, path), {
    ...init,
    method: 'DELETE',
    headers: withHeaders(ctx.headers, init),
  });
  if (!resp.ok) {
    throw new AdminApiError(resp.status, await parseErrorBody(resp));
  }
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
  const resp = await fetch(buildUrl(ctx.baseUrl, path), {
    ...init,
    method: 'PATCH',
    headers,
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    throw new AdminApiError(resp.status, await parseErrorBody(resp));
  }
  return (await resp.json()) as TResponse;
};
