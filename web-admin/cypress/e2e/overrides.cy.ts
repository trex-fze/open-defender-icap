const stubAuth = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
  win.localStorage.setItem(
    'od.admin.user',
    JSON.stringify({
      username: 'override-admin',
      name: 'Override Admin',
      email: 'overrides@example.com',
      roles: ['policy-admin'],
    }),
  );
};

describe('Overrides view', () => {
  beforeEach(() => {
    cy.intercept('GET', '**/api/v1/overrides**', {
      statusCode: 200,
      body: {
        data: [
          {
            id: 'ovr-1',
            scope_type: 'domain',
            scope_value: 'example.com',
            action: 'allow',
            status: 'active',
            reason: 'cypress',
            expires_at: null,
          },
        ],
        meta: { has_more: false, limit: 50 },
      },
    }).as('listOverrides');
    cy.visit('/overrides', { onBeforeLoad: stubAuth });
    cy.wait('@listOverrides');
  });

  it('renders overrides table', () => {
    cy.contains('Domain-level manual decisions').should('be.visible');
    cy.get('table tbody tr').should('have.length.greaterThan', 0);
    cy.get('table tbody tr').first().within(() => {
      cy.contains(/active|pending/i).should('exist');
    });
  });

  it('passes axe scan (serious+)', () => {
    cy.injectAxe();
    cy.checkA11y('.glass-panel', {
      includedImpacts: ['serious', 'critical'],
    });
  });
});
