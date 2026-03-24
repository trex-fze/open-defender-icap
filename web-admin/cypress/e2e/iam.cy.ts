export {};

const setAuthToken = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
};

const stubIamApi = () => {
  const users = [
    {
      user: {
        id: '0c2f2b71-9ab6-4f39-905a-0b2d4f0a1111',
        email: 'avery@example.com',
        display_name: 'Avery Quinn',
        subject: null,
        status: 'active',
        created_at: '2026-03-24T00:00:00Z',
        updated_at: '2026-03-24T00:00:00Z',
        last_login_at: null,
      },
      roles: ['policy-admin'],
      groups: [],
    },
  ];
  const groups = [
    {
      group: {
        id: 'a8e60a2d-9694-4da1-8f1c-5f8f8cfeabcd',
        name: 'Global Admins',
        description: 'Full control',
        status: 'active',
        created_at: '2026-03-01T00:00:00Z',
        updated_at: '2026-03-01T00:00:00Z',
      },
      members: users.map((entry) => entry.user),
      roles: ['policy-admin'],
    },
  ];
  const roles = [
    {
      id: '00000000-0000-0000-0000-000000000101',
      name: 'policy-admin',
      description: 'Full administrative access',
      builtin: true,
      created_at: '2026-03-24T00:00:00Z',
      permissions: ['iam:manage', 'policy:edit'],
    },
    {
      id: '00000000-0000-0000-0000-000000000102',
      name: 'policy-editor',
      description: 'Policy authoring',
      builtin: true,
      created_at: '2026-03-24T00:00:00Z',
      permissions: ['policy:edit'],
    },
  ];
  const serviceAccounts = [
    {
      account: {
        id: '32afc19d-1e6f-4b6b-8a04-6ee430424242',
        name: 'deploy-bot',
        description: 'CI/CD deploy pipeline',
        status: 'active',
        token_hint: 'xyz12345',
        created_at: '2026-03-24T00:00:00Z',
        last_rotated_at: '2026-03-24T00:00:00Z',
      },
      roles: ['policy-editor'],
    },
  ];
  const audit = [
    {
      id: 'c2446a27-9b56-47b8-9df3-2c8e2f6bb001',
      actor: 'alice@example.com',
      action: 'iam.user.create',
      target_type: 'user',
      target_id: '0c2f2b71-9ab6-4f39-905a-0b2d4f0a1111',
      payload: { email: 'avery@example.com' },
      created_at: '2026-03-24T00:00:00Z',
    },
  ];

  cy.intercept('GET', '**/api/v1/iam/users', users).as('listUsers');
  cy.intercept('GET', '**/api/v1/iam/roles', roles).as('listRoles');
  cy.intercept('GET', '**/api/v1/iam/groups', groups).as('listGroups');
  cy.intercept('GET', '**/api/v1/iam/service-accounts', serviceAccounts).as('listServiceAccounts');
  cy.intercept('GET', '**/api/v1/iam/audit', audit).as('listAudit');
  cy.intercept('POST', '**/api/v1/iam/service-accounts', {
    account: serviceAccounts[0].account,
    token: 'svc.token.value',
    roles: ['policy-editor'],
  }).as('createServiceAccount');
};

describe('IAM workspace', () => {
  beforeEach(() => {
    stubIamApi();
    cy.visit('/settings/iam', { onBeforeLoad: setAuthToken });
    cy.wait(['@listUsers', '@listRoles']);
  });

  it('renders the directory view', () => {
    cy.contains('IAM Workspace').should('be.visible');
    cy.contains('Avery Quinn').should('be.visible');
    cy.get('table tbody tr').should('have.length.at.least', 1);
    cy.contains('policy-admin').should('be.visible');
  });

  it('switches to groups tab', () => {
    cy.contains('Groups').click();
    cy.wait('@listGroups');
    cy.contains('Global Admins').should('be.visible');
    cy.contains('Add member…').should('exist');
  });

  it('creates a service account and shows the token', () => {
    cy.contains('Service Accounts').click();
    cy.wait('@listServiceAccounts');
    cy.get('input[placeholder="policy-ci"]').clear().type('deploy-bot');
    cy.contains('Generate Token').click();
    cy.wait('@createServiceAccount');
    cy.contains('svc.token.value').should('be.visible');
  });
});
