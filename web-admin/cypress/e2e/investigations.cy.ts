const seedInvestigationsAuth = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
  win.localStorage.setItem(
    'od.admin.user',
    JSON.stringify({
      username: 'investigator',
      name: 'Investigator',
      email: 'investigator@example.com',
      roles: ['policy-viewer'],
    }),
  );
};

describe('Investigations view', () => {
  beforeEach(() => {
    cy.intercept('GET', '**/api/v1/classifications/pending**', {
      statusCode: 200,
      body: {
        data: [
          {
            normalized_key: 'url:https://example.com/path',
            status: 'pending',
            base_url: 'https://example.com/path',
            requested_at: '2026-03-25T00:00:00Z',
            updated_at: '2026-03-25T00:00:00Z',
          },
        ],
        meta: { has_more: false, limit: 50 },
      },
    }).as('pending');
    cy.visit('/investigations', { onBeforeLoad: seedInvestigationsAuth });
    cy.wait('@pending');
  });

  it('filters normalized keys', () => {
    cy.contains('Pending Classification Investigations').should('be.visible');
    cy.get('.search-input').type('url:');
    cy.get('table tbody tr').should('have.length.greaterThan', 0);
  });

  it('passes axe scan (serious+)', () => {
    cy.injectAxe();
    cy.checkA11y('.glass-panel', {
      includedImpacts: ['serious', 'critical'],
    });
  });
});
