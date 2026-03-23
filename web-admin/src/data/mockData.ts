export const kpis = [
  { label: 'Requests Screened', value: '8.2M', change: '+4.1%', tone: 'green' },
  { label: 'LLM Queue Latency', value: '620ms', change: '-120ms', tone: 'green' },
  { label: 'Review SLA', value: '92%', change: '-3%', tone: 'amber' },
  { label: 'Escalations', value: '28', change: '+6', tone: 'red' },
];

export const investigations = [
  {
    key: 'domain:glowfeed.social',
    verdict: 'Warn',
    risk: 'medium',
    lastSeen: '2m ago',
    tags: ['Social', 'New domain'],
  },
  {
    key: 'url:https://alpha-malware.dark/doorway',
    verdict: 'Block',
    risk: 'critical',
    lastSeen: '17m ago',
    tags: ['Malware', 'LLM escalation'],
  },
];

type PolicyRule = {
  id: string;
  description?: string;
  priority: number;
  action: string;
  conditions: Record<string, unknown>;
};

export type PolicySummary = {
  id: string;
  name: string;
  version: string;
  status: 'active' | 'draft';
  rules: PolicyRule[];
};

export const policies: PolicySummary[] = [
  {
    id: 'pol-001',
    name: 'Global Secure Browsing',
    version: 'release-20250315',
    status: 'active',
    rules: [
      {
        id: 'block-c2',
        description: 'Block command & control endpoints',
        priority: 5,
        action: 'Block' as any,
        conditions: { categories: ['C2 Infrastructure'] },
      },
      {
        id: 'warn-new-social',
        description: 'Warn new social domains',
        priority: 25,
        action: 'Warn' as any,
        conditions: { categories: ['Social'], domains: ['*.new'] },
      },
    ],
  },
  {
    id: 'pol-002',
    name: 'APAC Finance Segment',
    version: 'draft-20250319',
    status: 'draft',
    rules: [
      {
        id: 'allow-banks',
        description: 'Allow core banking portals',
        priority: 10,
        action: 'Allow' as any,
        conditions: { domains: ['*.bank.example'] },
      },
    ],
  },
];

export const reviewQueue = [
  { id: 'rev-551', key: 'url:https://mirror.xyz', status: 'open', sla: '2h', risk: 'medium' },
  { id: 'rev-552', key: 'domain:orion-news.io', status: 'open', sla: '25m', risk: 'low' },
  { id: 'rev-553', key: 'url:https://scarlet-harbor.cn', status: 'urgent', sla: '5m', risk: 'critical' },
];

export const overrides = [
  { id: 'ovr-100', scope: 'domain:partner.example', action: 'allow', expires: 'in 7 days', status: 'active' },
  { id: 'ovr-101', scope: 'user:ceo@example.com', action: 'require-approval', expires: 'never', status: 'active' },
];

export const taxonomy = {
  categories: [
    {
      id: 'cat-social',
      name: 'Social Media',
      defaultAction: 'Warn',
      subcategories: [
        { id: 'sub-short', name: 'Short form video', defaultAction: 'Warn' },
        { id: 'sub-forums', name: 'Forums', defaultAction: 'Monitor' },
      ],
    },
    {
      id: 'cat-malware',
      name: 'Malware',
      defaultAction: 'Block',
      subcategories: [
        { id: 'sub-c2', name: 'Command & Control', defaultAction: 'Block' },
        { id: 'sub-dropper', name: 'Dropper', defaultAction: 'Block' },
      ],
    },
  ],
};

export const reports = [
  {
    id: 'rep-1',
    dimension: 'category',
    period: '24h',
    metrics: {
      Allow: 420000,
      Block: 58000,
      Warn: 12000,
      Review: 3200,
    },
  },
];

export const rbacMatrix = [
  { name: 'Avery Quinn', email: 'avery@example.com', roles: ['policy-admin', 'review-approver'] },
  { name: 'Blair Soto', email: 'blair@example.com', roles: ['policy-editor', 'policy-viewer'] },
  { name: 'Casey Lin', email: 'casey@example.com', roles: ['auditor'] },
];
