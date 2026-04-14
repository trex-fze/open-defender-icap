const seedDashboardAuth = (win: Window) => {
  win.localStorage.setItem('od.admin.tokens', JSON.stringify({ accessToken: 'demo-token' }));
  win.localStorage.setItem(
    'od.admin.user',
    JSON.stringify({
      username: 'dashboard-user',
      name: 'Dashboard User',
      email: 'dashboard@example.com',
      roles: ['policy-admin', 'policy-viewer', 'auditor'],
    }),
  );
};

describe('Dashboard analytics', () => {
  beforeEach(() => {
    cy.intercept('GET', '**/api/v1/iam/whoami', {
      statusCode: 200,
      body: {
        actor: 'dashboard-user',
        roles: ['policy-admin', 'policy-viewer', 'auditor'],
        username: 'dashboard-user',
        email: 'dashboard@example.com',
        display_name: 'Dashboard User',
        must_change_password: false,
      },
    }).as('whoami');

    cy.intercept('GET', '**/api/v1/classifications/pending**', {
      statusCode: 200,
      body: {
        data: [{ normalized_key: 'domain:example.com' }],
        meta: { has_more: false, limit: 500 },
      },
    }).as('pending');

    cy.intercept('GET', '**/api/v1/reporting/dashboard**', {
      statusCode: 200,
      body: {
        range: '24h',
        bucket_interval: '1h',
        overview: {
          total_requests: 520,
          allow_requests: 470,
          blocked_requests: 50,
          block_rate: 0.096,
          unique_clients: 33,
          total_bandwidth_bytes: 123456789,
        },
        hourly_usage: [
          {
            timestamp: '2026-04-10T00:00:00.000Z',
            total_requests: 50,
            blocked_requests: 5,
            bandwidth_bytes: 1548576,
          },
          {
            timestamp: '2026-04-10T01:00:00.000Z',
            total_requests: 40,
            blocked_requests: 4,
            bandwidth_bytes: 1048576,
          },
        ],
        top_domains: [{ key: 'google.com', doc_count: 140 }],
        top_blocked_domains: [{ key: 'youtube.com', doc_count: 22 }],
        top_blocked_requesters: [{ key: '192.168.1.253', doc_count: 12 }],
        top_clients_by_bandwidth: [{ key: '192.168.1.253', doc_count: 81, bandwidth_bytes: 40265318 }],
        coverage: {
          total_docs: 520,
          client_ip_docs: 520,
          domain_docs: 499,
          network_bytes_docs: 510,
        },
      },
    }).as('dashboardReport');

    cy.intercept('GET', '**/api/v1/reporting/ops-llm-series**', {
      statusCode: 200,
      body: {
        range: '1h',
        source: 'live',
        step_seconds: 15,
        providers: [
          {
            provider: 'local-lmstudio',
            success: [{ ts_ms: 1712707200000, value: 30 }],
            failures: [{ ts_ms: 1712707200000, value: 4 }],
            timeouts: [{ ts_ms: 1712707200000, value: 1 }],
            non_retryable_400: [{ ts_ms: 1712707200000, value: 2 }],
          },
        ],
        errors: [],
      },
    }).as('llmSeries');

    cy.visit('/dashboard', { onBeforeLoad: seedDashboardAuth });
    cy.wait('@whoami');
    cy.wait('@pending');
    cy.wait('@dashboardReport');
    cy.wait('@llmSeries');
  });

  it('renders rich dashboard analytics panels', () => {
    cy.contains('Trust & Safety Pulse').should('be.visible');
    cy.contains('Unique Clients').should('be.visible');
    cy.contains('Summed proxy payload bytes').should('be.visible');
    cy.contains('Top 10 clients shown').should('be.visible');
    cy.contains('Blocked Domains').should('be.visible');
    cy.contains('Top Requesters of Blocked Domains').should('be.visible');
    cy.contains('LLM Outcomes (Prometheus Series)').should('be.visible');
    cy.contains('Non-retryable HTTP 400').should('be.visible');
    cy.get('select').contains('1m').should('exist');
    cy.get('select').contains('5m').should('exist');
    cy.get('select').contains('15m').should('exist');
    cy.contains('192.168.1.253').should('be.visible');
  });

  it('passes axe scan (serious+)', () => {
    cy.injectAxe();
    cy.checkA11y('.glass-panel', {
      includedImpacts: ['serious', 'critical'],
    });
  });
});
