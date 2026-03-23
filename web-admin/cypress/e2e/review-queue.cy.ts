const mockAuth = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
};

describe('Review queue view', () => {
  beforeEach(() => {
    cy.visit('/review-queue', { onBeforeLoad: mockAuth });
  });

  it('shows review table rows with SLA chips', () => {
    cy.contains('Human-in-the-loop decisions').should('be.visible');
    cy.get('table tbody tr').should('have.length.greaterThan', 0);
    cy.get('table tbody tr').first().within(() => {
      cy.contains(/open|urgent|resolved/i).should('exist');
    });
  });

  it('passes axe scan (critical)', () => {
    cy.injectAxe();
    cy.checkA11y('.glass-panel', {
      includedImpacts: ['critical'],
    });
  });
});
