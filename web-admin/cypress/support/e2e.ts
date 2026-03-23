import 'cypress-axe';

beforeEach(() => {
  cy.window().then((win) => {
    win.localStorage.clear();
  });
});
