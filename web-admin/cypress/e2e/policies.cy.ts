const seedAuth = (win: Window, roles: Array<'policy-admin' | 'policy-editor' | 'policy-viewer'>) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
  win.localStorage.setItem(
    'od.admin.user',
    JSON.stringify({
      username: 'cypress-admin',
      name: 'Cypress Admin',
      email: 'cypress@example.com',
      roles,
    }),
  );
};

const mockWhoAmI = (roles: string[] = ['policy-admin']) => {
  cy.intercept('GET', '**/api/v1/iam/whoami', {
    statusCode: 200,
    body: {
      actor: 'cypress-admin',
      roles,
      username: 'cypress-admin',
      email: 'cypress@example.com',
      display_name: 'Cypress Admin',
      must_change_password: false,
    },
  }).as('whoami');
};

describe('Policies lifecycle', () => {
  it('shows policy rows and role-aware actions', () => {
    mockWhoAmI(['policy-admin']);
    cy.intercept('GET', '**/api/v1/policies**', {
      statusCode: 200,
      body: {
        data: [
          { id: 'p-active', name: 'Active Policy', version: 'release-1', status: 'active', rule_count: 3 },
          { id: 'p-draft', name: 'Draft Policy', version: 'draft-1', status: 'draft', rule_count: 2 },
        ],
        meta: { has_more: false, limit: 50 },
      },
    }).as('listPolicies');

    cy.visit('/policies', {
      onBeforeLoad: (win) => seedAuth(win, ['policy-admin']),
    });

    cy.wait('@whoami');
    cy.wait('@listPolicies');

    cy.contains('Decision Templates').should('be.visible');
    cy.contains('New Draft').should('be.visible');
    cy.get('table tbody tr').should('have.length', 2);
    cy.contains('tr', 'Draft Policy').within(() => {
      cy.contains('Activate').should('exist');
      cy.contains('Disable').should('exist');
      cy.contains('Delete').should('exist');
    });
  });

  it('activates a draft policy from the list', () => {
    mockWhoAmI(['policy-admin']);
    cy.intercept('GET', '**/api/v1/policies**', {
      statusCode: 200,
      body: {
        data: [{ id: 'p-draft', name: 'Draft Policy', version: 'draft-1', status: 'draft', rule_count: 2 }],
        meta: { has_more: false, limit: 50 },
      },
    }).as('listPolicies');
    cy.intercept('POST', '**/api/v1/policies/p-draft/publish', {
      statusCode: 200,
      body: {
        id: 'p-draft',
        name: 'Draft Policy',
        version: 'release-2',
        status: 'active',
        rule_count: 2,
        rules: [],
      },
    }).as('publishPolicy');

    cy.visit('/policies', {
      onBeforeLoad: (win) => seedAuth(win, ['policy-admin']),
    });

    cy.wait('@whoami');
    cy.wait('@listPolicies');
    cy.contains('tr', 'Draft Policy').within(() => {
      cy.contains('Activate').click();
    });
    cy.wait('@publishPolicy')
      .its('request.body')
      .should((body) => {
        expect(body.notes).to.contain('Activated via web-admin');
      });
    cy.contains('Policy Draft Policy activated.').should('be.visible');
  });

  it('validates, disables, and hard deletes a non-active policy in detail view', () => {
    mockWhoAmI(['policy-admin']);
    cy.intercept('GET', '**/api/v1/policies/p-draft', {
      statusCode: 200,
      body: {
        id: 'p-draft',
        name: 'Draft Policy',
        version: 'draft-7',
        status: 'draft',
        rule_count: 1,
        rules: [
          {
            id: 'rule-1',
            description: 'Allow default',
            priority: 100,
            action: 'Allow',
            conditions: {},
          },
        ],
      },
    }).as('policyDetail');
    cy.intercept('GET', '**/api/v1/policies/p-draft/versions', {
      statusCode: 200,
      body: [
        {
          id: 'v-1',
          policy_id: 'p-draft',
          version: 'draft-7',
          status: 'draft',
          created_by: 'cypress-admin',
          created_at: '2026-04-09T10:00:00Z',
          deployed_at: null,
          notes: 'seed',
          rule_count: 1,
        },
      ],
    }).as('policyVersions');
    cy.intercept('POST', '**/api/v1/policies/validate', {
      statusCode: 200,
      body: { valid: true, errors: [] },
    }).as('validatePolicy');
    cy.intercept('PUT', '**/api/v1/policies/p-draft', {
      statusCode: 200,
      body: {
        id: 'p-draft',
        name: 'Draft Policy',
        version: 'draft-7',
        status: 'archived',
        rule_count: 1,
        rules: [],
      },
    }).as('updatePolicy');
    cy.intercept('DELETE', '**/api/v1/policies/p-draft', {
      statusCode: 204,
      body: '',
    }).as('deletePolicy');
    cy.intercept('GET', '**/api/v1/policies**', {
      statusCode: 200,
      body: {
        data: [],
        meta: { has_more: false, limit: 50 },
      },
    }).as('listPolicies');

    cy.on('window:confirm', () => true);

    cy.visit('/policies/p-draft', {
      onBeforeLoad: (win) => seedAuth(win, ['policy-admin']),
    });

    cy.wait('@whoami');
    cy.wait('@policyDetail');
    cy.wait('@policyVersions');

    cy.contains('Validate').click();
    cy.wait('@validatePolicy')
      .its('request.body')
      .should((body) => {
        expect(body.name).to.eq('Draft Policy');
        expect(body.rules).to.have.length(1);
      });
    cy.contains('Validation passed. Policy rules are valid.').should('be.visible');

    cy.contains('Disable').click();
    cy.wait('@updatePolicy')
      .its('request.body')
      .should((body) => {
        expect(body.status).to.eq('archived');
      });

    cy.contains('Hard Delete').click();
    cy.wait('@deletePolicy');
    cy.url().should('include', '/policies');
  });

  it('disables destructive actions on active policy detail', () => {
    mockWhoAmI(['policy-admin']);
    cy.intercept('GET', '**/api/v1/policies/p-active', {
      statusCode: 200,
      body: {
        id: 'p-active',
        name: 'Active Policy',
        version: 'release-77',
        status: 'active',
        rule_count: 1,
        rules: [
          {
            id: 'rule-1',
            description: 'Block malware',
            priority: 10,
            action: 'Block',
            conditions: { categories: ['Malware / Phishing / Fraud'] },
          },
        ],
      },
    }).as('policyDetailActive');
    cy.intercept('GET', '**/api/v1/policies/p-active/versions', {
      statusCode: 200,
      body: [],
    }).as('policyVersionsActive');

    cy.visit('/policies/p-active', {
      onBeforeLoad: (win) => seedAuth(win, ['policy-admin']),
    });

    cy.wait('@whoami');
    cy.wait('@policyDetailActive');
    cy.wait('@policyVersionsActive');
    cy.contains('Disable').should('be.disabled');
    cy.contains('Hard Delete').should('be.disabled');
  });

  it('passes axe scan', () => {
    mockWhoAmI(['policy-admin']);
    cy.intercept('GET', '**/api/v1/policies**', {
      statusCode: 200,
      body: {
        data: [{ id: 'p-active', name: 'Active Policy', version: 'release-1', status: 'active', rule_count: 1 }],
        meta: { has_more: false, limit: 50 },
      },
    }).as('listPolicies');
    cy.visit('/policies', {
      onBeforeLoad: (win) => seedAuth(win, ['policy-admin']),
    });
    cy.wait('@whoami');
    cy.wait('@listPolicies');
    cy.injectAxe();
    cy.checkA11y('.glass-panel', {
      includedImpacts: ['serious', 'critical'],
    });
  });
});
