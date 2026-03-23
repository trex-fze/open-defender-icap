describe('Accessibility smoke', () => {
  it('passes axe-core scan on login page', () => {
    cy.visit('/login');
    cy.injectAxe();
    cy.checkA11y(null, {
      includedImpacts: ['serious', 'critical'],
    });
  });
});
