describe('Accessibility smoke', () => {
  const seedAuth = (win: Window) => {
    win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
  };

  it('passes axe-core scan on login page', () => {
    cy.visit('/login');
    cy.injectAxe();
    cy.checkA11y(undefined, {
      includedImpacts: ['serious', 'critical'],
    });
  });

  [
    '/policies/new',
    '/review-queue',
    '/overrides',
    '/taxonomy',
    '/classifications/pending',
    '/reports',
    '/diagnostics/page-content',
    '/diagnostics/cache',
    '/settings/rbac',
  ].forEach((route) => {
    it(`passes axe-core scan on ${route}`, () => {
      cy.visit(route, { onBeforeLoad: seedAuth });
      cy.injectAxe();
      cy.checkA11y('.main-panel', {
        includedImpacts: ['serious', 'critical'],
      });
    });
  });
});
