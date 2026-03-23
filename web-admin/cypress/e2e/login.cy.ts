describe('Login page', () => {
  it('shows mock device flow form', () => {
    cy.visit('/login');
    cy.contains(/OIDC Sign-in/i).should('be.visible');
    cy.get('#login-email').clear().type('analyst@example.com');
    cy.contains('button', /Continue/i).click();
    cy.window()
      .its('localStorage')
      .invoke('getItem', 'od.admin.tokens')
      .should('match', /accessToken/);
  });
});
