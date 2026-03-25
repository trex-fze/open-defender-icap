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

export type PolicyRule = {
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

export const taxonomyActivation = {
  version: 'mock-2026-03-20',
  updatedAt: '2026-03-20T15:04:00Z',
  updatedBy: 'system',
  categories: [
    {
      id: 'social-media',
      name: 'Social Media',
      enabled: true,
      locked: false,
      subcategories: [
        { id: 'short-video-platforms', name: 'Short-video platforms', enabled: true, locked: false },
        { id: 'social-networks', name: 'Social networks', enabled: true, locked: false },
        { id: 'creator-platforms', name: 'Creator platforms', enabled: false, locked: false },
      ],
    },
    {
      id: 'malware-phishing-fraud',
      name: 'Malware / Phishing / Fraud',
      enabled: false,
      locked: false,
      subcategories: [
        { id: 'phishing-sites', name: 'Phishing sites', enabled: false, locked: false },
        { id: 'malware-delivery', name: 'Malware delivery', enabled: false, locked: false },
        { id: 'credential-theft', name: 'Credential theft', enabled: false, locked: false },
      ],
    },
    {
      id: 'unknown-unclassified',
      name: 'Unknown / Unclassified',
      enabled: true,
      locked: true,
      subcategories: [
        { id: 'newly-seen-unknowns', name: 'Newly seen unknowns', enabled: true, locked: true },
        { id: 'insufficient-evidence', name: 'Insufficient evidence', enabled: true, locked: true },
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
