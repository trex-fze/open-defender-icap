describe('Login page', () => {
  it('shows mock device flow form', () => {
    cy.intercept('POST', '**/api/v1/auth/login', {
      statusCode: 200,
      body: {
        access_token: 'demo-access-token',
        refresh_token: 'demo-refresh-token',
        expires_in: 3600,
        user: {
          username: 'analyst',
          email: 'analyst@example.com',
          display_name: 'Analyst User',
          roles: ['policy-viewer'],
          must_change_password: false,
        },
      },
    }).as('login');
    cy.visit('/login');
    cy.contains(/Local Sign-in/i).should('be.visible');
    cy.get('#login-username').clear().type('analyst@example.com');
    cy.get('#login-password').clear().type('demo-password');
    cy.contains('button', /Continue/i).click();
    cy.wait('@login');
    cy.window()
      .its('localStorage')
      .invoke('getItem', 'od.admin.tokens')
      .should('match', /accessToken/);
  });
});
