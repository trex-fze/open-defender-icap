import '@testing-library/jest-dom/vitest';

// Reset localStorage between tests to avoid state bleed
afterEach(() => {
  localStorage.clear();
});
