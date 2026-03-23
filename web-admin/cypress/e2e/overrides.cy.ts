const stubAuth = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
};

describe('Overrides view', () => {
  beforeEach(() => {
    cy.visit('/overrides', { onBeforeLoad: stubAuth });
  });

  it('renders overrides table', () => {
    cy.contains('Manual policy exceptions').should('be.visible');
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
