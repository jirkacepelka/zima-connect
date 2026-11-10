import { invoke } from '@tauri-apps/api/core';

export type Cap = 'files' | 'clipboard' | 'notifications' | 'ring' | 'calls';
export const CAPS: { id: Cap; label: string; hint: string }[] = [
  { id: 'files', label: 'Files and photos', hint: 'Send files' },
  { id: 'clipboard', label: 'Clipboard', hint: 'Send clipboard text' },
  { id: 'notifications', label: 'Notifications', hint: 'Show notifications' },
  { id: 'ring', label: 'Find my device', hint: 'Make the device ring' },
  { id: 'calls', label: 'Calls (coming soon)', hint: 'Voice and video calls' }
];

export type Contact = {
  id: string;
  name: string;
  platform: string;
  user_id: string | null;
  user_name: string | null;
  mine: boolean;
  direct: boolean;
  caps: Cap[];
  online: boolean;
  rtt_ms: number | null;
};

export type Activity = {
  id: number;
  ts: number;
  kind: string;
  title: string;
  detail: string;
  ok: boolean;
  path: string | null;
};

export type User = { id: string; name: string; role: 'owner' | 'member' };
export type Device = { id: string; user_id: string; name: string; platform: string };
export type Rule = { from_user: string; to_user: string; cap: Cap; allow: boolean };
export type Household = {
  info: { household: string; version: number; me: User; users: User[]; can_admin: boolean };
  snapshot: { household: string; users: User[]; devices: Device[]; rules: Rule[] };
};

export type AdminOp =
  | { op: 'create_invite'; user_id: string | null; new_user_name: string | null }
  | { op: 'set_rule'; from_user: string; to_user: string; cap: Cap; allow: boolean }
  | { op: 'clear_rule'; from_user: string; to_user: string; cap: Cap }
  | { op: 'remove_device'; id: string }
  | { op: 'remove_user'; id: string }
  | { op: 'rename_user'; id: string; name: string };

export const api = {
  identity: () => invoke<{ id: string; name: string; platform: string }>('get_identity'),
  contacts: () => invoke<Contact[]>('get_contacts'),
  household: () => invoke<Household | null>('get_household'),
  activities: () => invoke<Activity[]>('get_activities'),
  join: (code: string, userName: string | null) =>
    invoke<string>('join_household', { code, userName }),
  leave: () => invoke<void>('leave_household'),
  admin: (op: AdminOp) => invoke<string | null>('household_admin', { op }),
  pairingCode: () => invoke<string>('create_pairing_code'),
  pair: (code: string) => invoke<string>('pair_device', { code }),
  unpair: (id: string) => invoke<void>('unpair_device', { id }),
  sendClipboard: (peerId: string) => invoke<void>('send_clipboard', { peerId }),
  sendFile: (peerId: string) => invoke<boolean>('send_file', { peerId }),
  ring: (peerId: string) => invoke<void>('ring_device', { peerId }),
  reveal: (path: string) => invoke<void>('reveal_path', { path })
};

export const errText = (e: unknown) => String(e).replace(/^Error:\s*/, '');

/** Same default as the Rust side: own devices all allowed, between people only files. */
export function allowed(h: Household['snapshot'], from: string, to: string, cap: Cap): boolean {
  const r = h.rules.find((x) => x.from_user === from && x.to_user === to && x.cap === cap);
  if (r) return r.allow;
  return from === to || cap === 'files';
}

export function defaultAllowed(from: string, to: string, cap: Cap): boolean {
  return from === to || cap === 'files';
}
