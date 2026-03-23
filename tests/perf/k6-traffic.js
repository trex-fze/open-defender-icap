import http from 'k6/http';
import { check, sleep } from 'k6';

const BASE_URL = __ENV.BASE_URL || 'http://localhost:19000';
const TOKEN = __ENV.ADMIN_TOKEN || 'changeme-admin';

export const options = {
  stages: [
    { duration: '30s', target: 20 },
    { duration: '2m', target: 20 },
    { duration: '30s', target: 0 },
  ],
  thresholds: {
    http_req_failed: ['rate<0.01'],
    http_req_duration: ['p(95)<500'],
  },
};

export default function () {
  const headers = {
    'X-Admin-Token': TOKEN,
  };

  const trafficRes = http.get(
    `${BASE_URL}/api/v1/reporting/traffic?range=1h&top_n=5`,
    { headers },
  );
  check(trafficRes, {
    'traffic 200': (r) => r.status === 200,
  });

  const policyRes = http.get(
    `${BASE_URL}/api/v1/policies?include_drafts=true&page_size=50`,
    { headers },
  );
  check(policyRes, {
    'policies 200': (r) => r.status === 200,
  });

  sleep(1);
}
