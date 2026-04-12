const isHttpUrl = (value: string) => value.toLowerCase().startsWith('http://');

export const resolveAdminApiBase = (): string => {
  const runtimeOverride =
    typeof window !== 'undefined'
      ? (window as Window & { __OD_ADMIN_API_URL__?: string }).__OD_ADMIN_API_URL__
      : undefined;
  const envUrl = (import.meta.env.VITE_ADMIN_API_URL ?? '').trim();
  const fallbackEnv = (import.meta.env.VITE_ADMIN_API_FALLBACK ?? '').trim();

  if (typeof window !== 'undefined') {
    const sameOrigin = window.location.origin;
    const configured = (runtimeOverride ?? envUrl).trim();
    if (configured) {
      if (window.location.protocol === 'https:' && isHttpUrl(configured)) {
        console.warn(
          'Ignoring insecure VITE_ADMIN_API_URL in HTTPS context; using same-origin /api proxy',
        );
        return sameOrigin;
      }
      return configured;
    }

    if (fallbackEnv) {
      if (window.location.protocol === 'https:' && isHttpUrl(fallbackEnv)) {
        console.warn(
          'Ignoring insecure VITE_ADMIN_API_FALLBACK in HTTPS context; using same-origin /api proxy',
        );
        return sameOrigin;
      }
      return fallbackEnv;
    }

    return sameOrigin;
  }

  return runtimeOverride?.trim() || envUrl || fallbackEnv || 'http://localhost:19000';
};
