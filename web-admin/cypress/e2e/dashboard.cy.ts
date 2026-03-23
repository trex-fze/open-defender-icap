const setAuthBeforeLoad = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
};

describe('Dashboard flow', () => {
  beforeEach(() => {
    cy.visit('/dashboard', { onBeforeLoad: setAuthBeforeLoad });
  });

  it('renders KPI cards and navigation', () => {
    cy.contains('Trust & Safety Pulse').should('be.visible');
    cy.contains('Requests Screened').should('be.visible');
    cy.get('aside').within(() => {
      cy.contains('Policies').should('exist');
      cy.contains('Review Queue').should('exist');
    });
  });

  it('passes axe on main panel (critical impact)', () => {
    cy.injectAxe();
    cy.checkA11y('.main-panel', {
      includedImpacts: ['critical'],
    });
  });
});
