const seedReportsAuth = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
};

describe('Reports view', () => {
  beforeEach(() => {
    cy.visit('/reports', { onBeforeLoad: seedReportsAuth });
  });

  it('shows KPI cards and table rows', () => {
    cy.contains('Reporting').should('be.visible');
    cy.get('.kpi-card').should('have.length.greaterThan', 0);
    cy.get('table tbody tr').should('have.length.greaterThan', 0);
  });

  it('passes axe scan (critical)', () => {
    cy.injectAxe();
    cy.checkA11y('.glass-panel', {
      includedImpacts: ['critical'],
    });
  });
});
