export type IamUserRecord = {
  id: string;
  username?: string | null;
  subject?: string | null;
  email?: string | null;
  display_name?: string | null;
  status: string;
  last_login_at?: string | null;
  created_at: string;
  updated_at: string;
};

export type IamGroupRecord = {
  id: string;
  name: string;
  description?: string | null;
  status: string;
  created_at: string;
  updated_at: string;
};

export type IamGroupDetails = {
  group: IamGroupRecord;
  members: IamUserRecord[];
  roles: string[];
};

export type IamUserDetails = {
  user: IamUserRecord;
  roles: string[];
  groups: IamGroupRecord[];
};

export type IamRoleRecord = {
  id: string;
  name: string;
  description?: string | null;
  builtin: boolean;
  created_at: string;
  permissions: string[];
};

export type ServiceAccountRecord = {
  id: string;
  name: string;
  description?: string | null;
  status: string;
  token_hint?: string | null;
  created_at: string;
  last_rotated_at?: string | null;
};

export type ServiceAccountDetails = {
  account: ServiceAccountRecord;
  roles: string[];
};

export type ServiceAccountWithToken = {
  account: ServiceAccountRecord;
  token: string;
  roles: string[];
};

export type IamAuditRecord = {
  id: string;
  actor?: string | null;
  action: string;
  target_type?: string | null;
  target_id?: string | null;
  payload?: Record<string, unknown> | null;
  created_at: string;
};
