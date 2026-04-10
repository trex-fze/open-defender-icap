const seedReportsAuth = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
  win.localStorage.setItem(
    'od.admin.user',
    JSON.stringify({
      username: 'report-user',
      name: 'Report User',
      email: 'reports@example.com',
      roles: ['auditor'],
    }),
  );
};

describe('Reports view', () => {
  beforeEach(() => {
    cy.intercept('GET', '**/api/v1/reporting/traffic**', {
      statusCode: 200,
      body: {
        range: '24h',
        bucket_interval: '1h',
        allow_block_trend: [
          {
            action: 'allow',
            buckets: [{ key_as_string: '2026-04-10T00:00:00.000Z', doc_count: 10 }],
          },
        ],
        top_blocked_domains: [{ key: 'blocked.example', doc_count: 4 }],
        top_categories: [{ key: 'social-media', doc_count: 11 }],
      },
    }).as('traffic');
    cy.intercept('GET', '**/api/v1/reporting/status**', {
      statusCode: 200,
      body: {
        range: '24h',
        total_docs: 100,
        action_docs: 99,
        category_docs: 98,
        domain_docs: 97,
      },
    }).as('status');
    cy.visit('/reports', { onBeforeLoad: seedReportsAuth });
    cy.wait('@traffic');
    cy.wait('@status');
  });

  it('shows KPI cards and table rows', () => {
    cy.contains('Reporting').should('be.visible');
    cy.get('.kpi-card').should('have.length.greaterThan', 0);
    cy.get('table tbody tr').should('have.length.greaterThan', 0);
  });

  it('passes axe scan (serious+)', () => {
    cy.injectAxe();
    cy.checkA11y('.glass-panel', {
      includedImpacts: ['serious', 'critical'],
    });
  });
});
