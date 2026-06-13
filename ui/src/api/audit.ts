import { apiFetch } from './client';

export type AuditChange = { field: string; from: string; to: string };

export interface AuditRow {
  id: string;
  action: string;
  category: 'content' | 'auth' | 'settings' | 'perm';
  status: 'success' | 'failed';
  actor_type: 'user' | 'api_token' | 'system';
  actor_id: string | null;
  actor_label: string;
  target_type: string | null;
  target_id: string | null;
  target_label: string | null;
  changes: AuditChange[] | null;
  note: string | null;
  ip: string | null;
  user_agent: string | null;
  request_id: string | null;
  created_at: string;
}

export interface AuditListResp {
  rows: AuditRow[];
  total: number;
  page: number;
  per_page: number;
  category_counts: Record<string, number>;
}

export interface AuditStats {
  events_logged: number;
  sign_ins: number;
  failed_attempts: number;
  content_changes: number;
  failed_actions: number;
}

export interface AuditFilters {
  category?: string;
  status?: string;
  actor_id?: string;
  q?: string;
  page?: number;
  per_page?: number;
}

function qs(f: AuditFilters): string {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(f)) {
    if (v !== undefined && v !== null && v !== '' && v !== 'all') {
      p.set(k, String(v));
    }
  }
  const s = p.toString();
  return s ? `?${s}` : '';
}

export function listAudit(f: AuditFilters): Promise<AuditListResp> {
  return apiFetch<AuditListResp>(`/api/admin/audit${qs(f)}`);
}

export function auditStats(): Promise<AuditStats> {
  return apiFetch<AuditStats>('/api/admin/audit/stats');
}

export function auditExportPath(f: AuditFilters): string {
  return `/api/admin/audit/export${qs(f)}`;
}
