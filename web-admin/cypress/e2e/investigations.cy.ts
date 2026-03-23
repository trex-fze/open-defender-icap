const seedInvestigationsAuth = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
};

describe('Investigations view', () => {
  beforeEach(() => {
    cy.visit('/investigations', { onBeforeLoad: seedInvestigationsAuth });
  });

  it('filters normalized keys', () => {
    cy.contains('Classification History & Cache').should('be.visible');
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
