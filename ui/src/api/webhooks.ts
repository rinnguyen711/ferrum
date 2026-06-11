import { apiFetch } from './client';

export interface Webhook {
  id: string;
  name: string;
  url: string;
  events: string[];
  enabled: boolean;
  created_at: string;
}

export interface WebhookDelivery {
  id: string;
  webhook_id: string;
  event: string;
  status: string;
  attempt: number;
  last_error: string | null;
  created_at: string;
}

export interface WebhookHeader {
  key: string;
  value: string;
}

export interface CreateWebhookBody {
  name: string;
  url: string;
  events: string[];
  secret?: string;
  headers?: WebhookHeader[];
}

export interface UpdateWebhookBody {
  name: string;
  url: string;
  events: string[];
  secret?: string;
  enabled: boolean;
}

export const WEBHOOK_EVENTS = [
  'entry.created',
  'entry.updated',
  'entry.deleted',
  'entry.published',
  'entry.unpublished',
] as const;

export type WebhookEvent = typeof WEBHOOK_EVENTS[number];

export function listWebhooks(): Promise<Webhook[]> {
  return apiFetch<Webhook[]>('/admin/webhooks');
}

export function getWebhook(id: string): Promise<Webhook> {
  return apiFetch<Webhook>(`/admin/webhooks/${encodeURIComponent(id)}`);
}

export function createWebhook(body: CreateWebhookBody): Promise<Webhook> {
  return apiFetch<Webhook>('/admin/webhooks', { method: 'POST', body });
}

export function updateWebhook(id: string, body: UpdateWebhookBody): Promise<Webhook> {
  return apiFetch<Webhook>(`/admin/webhooks/${encodeURIComponent(id)}`, { method: 'PATCH', body });
}

/** Toggle enabled on/off. The backend PATCH requires the full record, so
 *  carry over the webhook's current fields and flip only `enabled`. */
export function setWebhookEnabled(w: Webhook, enabled: boolean): Promise<Webhook> {
  return updateWebhook(w.id, {
    name: w.name,
    url: w.url,
    events: w.events,
    enabled,
  });
}

export function deleteWebhook(id: string): Promise<void> {
  return apiFetch<void>(`/admin/webhooks/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

export function listDeliveries(webhookId: string): Promise<WebhookDelivery[]> {
  return apiFetch<WebhookDelivery[]>(`/admin/webhooks/${encodeURIComponent(webhookId)}/deliveries`);
}

export function testWebhook(id: string): Promise<void> {
  return apiFetch<void>(`/admin/webhooks/${encodeURIComponent(id)}/test`, { method: 'POST' });
}
