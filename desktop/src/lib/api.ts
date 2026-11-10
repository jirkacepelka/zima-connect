import { invoke } from '@tauri-apps/api/core';

export type Delivery = 'delivered' | 'queued';
export type Cap = 'files' | 'clipboard' | 'notifications' | 'ring' | 'calls' | 'remote' | 'status';
export const CAPS: { id: Cap; label: string; hint: string }[] = [
  { id: 'files', label: 'Files and photos', hint: 'Send files' },
  { id: 'clipboard', label: 'Clipboard', hint: 'Send clipboard text' },
  { id: 'notifications', label: 'Notifications', hint: 'Show notifications' },
  { id: 'ring', label: 'Find my device', hint: 'Make the device ring' },
  { id: 'calls', label: 'Voice calls', hint: 'Call the device' },
  { id: 'remote', label: 'Remote control', hint: 'Touchpad, keyboard and media keys' },
  { id: 'status', label: 'Battery and status', hint: 'See the battery level' }
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
  battery: [number, boolean] | null;
};

export type Audience = { to: 'everyone' } | { to: 'users'; ids: string[] };
export type ShelfItem = {
  id: string;
  name: string;
  size: number;
  owner_user: string;
  owner_user_name: string;
  owner_device: string;
  audience: Audience;
  created: number;
  mine: boolean;
};
export type Settings = { background: boolean; autostart: boolean; allow_remote: boolean };
export type RemoteInput =
  | { k: 'move'; dx: number; dy: number }
  | { k: 'click'; button: 'left' | 'right' | 'middle' | 'double' }
  | { k: 'scroll'; dy: number }
  | { k: 'text'; text: string }
  | { k: 'key'; key: string }
  | { k: 'media'; action: 'play_pause' | 'next' | 'prev' | 'vol_up' | 'vol_down' | 'mute' };

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
  | { op: 'rename_user'; id: string; name: string }
  | { op: 'rename_device'; id: string; name: string }
  | { op: 'create_user'; name: string }
  | { op: 'set_role'; id: string; owner: boolean };

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
  sendClipboard: (peerId: string) => invoke<Delivery>('send_clipboard', { peerId }),
  sendFile: (peerId: string) => invoke<Delivery | null>('send_file', { peerId }),
  ring: (peerId: string) => invoke<void>('ring_device', { peerId }),
  ringStop: (peerId: string) => invoke<void>('ring_stop_device', { peerId }),
  ringState: () => invoke<string | null>('get_ring_state'),
  stopRing: () => invoke<void>('stop_ring'),
  reveal: (path: string) => invoke<void>('reveal_path', { path }),
  renameDevice: (name: string) => invoke<string>('rename_device', { name }),
  settings: () => invoke<Settings>('get_settings'),
  setSettings: (next: Settings) => invoke<Settings>('set_settings', { new: next }),
  shelfList: () => invoke<ShelfItem[]>('shelf_list'),
  shelfPut: (audience: Audience) => invoke<ShelfItem | null>('shelf_put', { audience }),
  shelfPutPath: (audience: Audience, path: string) => invoke<ShelfItem>('shelf_put_path', { audience, path }),
  shelfGet: (id: string) => invoke<string>('shelf_get', { id }),
  shelfDelete: (id: string) => invoke<void>('shelf_delete', { id }),
  sendUrl: (peerId: string, url: string) => invoke<void>('send_url', { peerId, url }),
  sendPath: (peerId: string, path: string) => invoke<Delivery>('send_path', { peerId, path }),
  remote: (peerId: string, input: RemoteInput) => invoke<void>('send_remote', { peerId, input })
};

export const errText = (e: unknown) => String(e).replace(/^Error:\s*/, '');

/** Same rule as the Rust side. */
export function allowed(h: Household['snapshot'], from: string, to: string, cap: Cap): boolean {
  const r = h.rules.find((x) => x.from_user === from && x.to_user === to && x.cap === cap);
  if (r) return r.allow;
  return defaultAllowed(from, to, cap);
}

/** Own devices: everything. Between people: things done on purpose (files, clipboard, find my device). */
export function defaultAllowed(from: string, to: string, cap: Cap): boolean {
  return from === to || cap === 'files' || cap === 'clipboard' || cap === 'ring';
}
