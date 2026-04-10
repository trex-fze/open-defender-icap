const seedAuthAndApi = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
  win.localStorage.setItem(
    'od.admin.user',
    JSON.stringify({
      username: 'stage10-admin',
      name: 'Stage10 Admin',
      email: 'stage10@example.com',
      roles: ['policy-admin'],
    }),
  );
  (win as Window & { __OD_ADMIN_API_URL__?: string }).__OD_ADMIN_API_URL__ =
    'http://127.0.0.1:19001';
};

describe('Stage 10 management parity flows', () => {
  it('creates a policy draft and publishes it', () => {
    cy.intercept('POST', '**/api/v1/policies', {
      statusCode: 200,
      body: { id: 'pol-stage10' },
    }).as('createPolicy');

    cy.intercept('GET', '**/api/v1/policies/pol-stage10', {
      statusCode: 200,
      body: {
        id: 'pol-stage10',
        name: 'Stage10 Draft',
        version: 'draft-1',
        status: 'draft',
        rules: [
          {
            id: 'starter-monitor',
            description: 'Starter monitor rule',
            priority: 100,
            action: 'Monitor',
            conditions: {},
          },
        ],
      },
    }).as('policyDetail');

    cy.intercept('GET', '**/api/v1/policies/pol-stage10/versions', {
      statusCode: 200,
      body: [],
    }).as('policyVersions');

    cy.intercept('POST', '**/api/v1/policies/pol-stage10/publish', {
      statusCode: 200,
      body: { ok: true },
    }).as('publishPolicy');

    cy.visit('/policies/new', { onBeforeLoad: seedAuthAndApi });
    cy.contains('Create New Draft').should('be.visible');
    cy.get('input[placeholder*="Corporate Safe Browsing Draft"]').type('Stage10 Draft');
    cy.contains('button', 'Create Draft').click();

    cy.wait('@createPolicy');
    cy.wait('@policyDetail');
    cy.contains('Policy Detail').should('be.visible');

    cy.contains('button', 'Activate').click();
    cy.wait('@publishPolicy');
    cy.contains(/activated successfully/i).should('be.visible');
  });

  it('applies manual decisions for pending classifications', () => {
    cy.intercept('GET', '**/api/v1/classifications/pending*', {
      statusCode: 200,
      body: {
        data: [
          {
            normalized_key: 'domain:stage10.example',
            status: 'pending',
            base_url: 'http://stage10.example/',
            updated_at: '2026-03-25T00:00:00Z',
            requested_at: '2026-03-25T00:00:00Z',
          },
        ],
        meta: { has_more: false, limit: 50 },
      },
    }).as('pendingList');

    cy.intercept('GET', '**/api/v1/taxonomy', {
      statusCode: 200,
      body: {
        version: '2026.03',
        categories: [
          {
            id: 'social-media',
            name: 'Social Media',
            enabled: true,
            locked: false,
            subcategories: [
              { id: 'social-networking', name: 'Social Networking', enabled: true, locked: false },
            ],
          },
        ],
      },
    }).as('taxonomy');

    cy.intercept('POST', '**/api/v1/classifications/domain%3Astage10.example/manual-classify', {
      statusCode: 200,
      body: { ok: true },
    }).as('manualClassify');

    cy.visit('/classifications/pending', { onBeforeLoad: seedAuthAndApi });
    cy.wait('@pendingList');
    cy.wait('@taxonomy');
    cy.contains('button', 'Manual Classify').click();
    cy.contains('button', 'Save Classification').click();
    cy.wait('@manualClassify');
    cy.contains(/saved classification for domain:stage10.example/i).should('be.visible');
  });

  it('runs diagnostics for page content and cache entries', () => {
    cy.intercept('GET', '**/api/v1/page-contents/domain%3Astage10.example*', {
      statusCode: 200,
      body: {
        normalized_key: 'domain:stage10.example',
        fetch_version: 2,
        fetch_status: 'fetched',
        ttl_seconds: 3600,
        fetched_at: '2026-03-25T00:00:00Z',
        expires_at: '2026-03-25T01:00:00Z',
        excerpt: 'Stage10 content excerpt',
        excerpt_truncated: false,
      },
    }).as('pageContentLatest');

    cy.intercept('GET', '**/api/v1/page-contents/domain%3Astage10.example/history*', {
      statusCode: 200,
      body: [
        {
          fetch_version: 2,
          fetch_status: 'fetched',
          ttl_seconds: 3600,
          fetched_at: '2026-03-25T00:00:00Z',
          expires_at: '2026-03-25T01:00:00Z',
          char_count: 128,
        },
      ],
    }).as('pageContentHistory');

    cy.visit('/diagnostics/page-content', { onBeforeLoad: seedAuthAndApi });
    cy.contains('span', 'Normalized key').parent().find('input').clear().type('domain:stage10.example');
    cy.contains('button', 'Lookup').click();
    cy.wait('@pageContentLatest');
    cy.wait('@pageContentHistory');
    cy.contains('Latest Content').should('be.visible');
    cy.contains('Version:').should('be.visible');

    cy.intercept('GET', '**/api/v1/cache-entries/domain%3Astage10.example', {
      statusCode: 200,
      body: {
        cache_key: 'domain:stage10.example',
        value: { action: 'Allow', source: 'classification' },
        expires_at: '2026-03-25T01:00:00Z',
        created_at: '2026-03-25T00:00:00Z',
      },
    }).as('cacheLookup');

    cy.intercept('DELETE', '**/api/v1/cache-entries/domain%3Astage10.example', {
      statusCode: 204,
      body: '',
    }).as('cacheDelete');

    cy.visit('/diagnostics/cache', { onBeforeLoad: seedAuthAndApi });
    cy.contains('span', 'Cache key').parent().find('input').clear().type('domain:stage10.example');
    cy.contains('button', 'Lookup').click();
    cy.wait('@cacheLookup');
    cy.contains('Result').should('be.visible');
    cy.contains('button', 'Evict').click();
    cy.wait('@cacheDelete');
    cy.contains(/deleted cache entry/i).should('be.visible');
  });
});
