const seedAuth = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
};

describe('Policies view', () => {
  beforeEach(() => {
    cy.visit('/policies', { onBeforeLoad: seedAuth });
  });

  it('lists policy rows with status chips', () => {
    cy.contains('Decision Templates').should('be.visible');
    cy.get('table tbody tr').should('have.length.greaterThan', 0);
    cy.get('table tbody tr').first().within(() => {
      cy.contains('View').should('exist');
    });
  });

  it('passes axe scan', () => {
    cy.injectAxe();
    cy.checkA11y('.glass-panel', {
      includedImpacts: ['serious', 'critical'],
    });
  });
});
