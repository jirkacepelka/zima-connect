import { invoke } from '@tauri-apps/api/core';

export type Delivery = 'delivered' | 'queued';
export type ScreenOp = { op: 'status' } | { op: 'video'; after: number } | { op: 'tap'; x: number; y: number } | { op: 'hold'; x: number; y: number } | { op: 'swipe'; x1: number; y1: number; x2: number; y2: number; ms: number } | { op: 'key'; name: 'back' | 'home' | 'recents' | 'notifications' };
export type PhoneOp = { op: 'dial'; number: string } | { op: 'answer' } | { op: 'decline' } | { op: 'hang_up' } | { op: 'mute'; on: boolean } | { op: 'state' };
export type Cap = 'files' | 'clipboard' | 'notifications' | 'ring' | 'calls' | 'remote' | 'status' | 'sms' | 'screen';
export const CAPS: { id: Cap; label: string; hint: string }[] = [
  { id: 'files', label: 'Files and photos', hint: 'Send files' },
  { id: 'clipboard', label: 'Clipboard', hint: 'Send clipboard text' },
  { id: 'notifications', label: 'Notifications', hint: 'Show notifications' },
  { id: 'ring', label: 'Find my device', hint: 'Make the device ring' },
  { id: 'calls', label: 'Phone calls', hint: 'Dial and answer through a phone' },
  { id: 'remote', label: 'Remote control', hint: 'Touchpad, keyboard and media keys' },
  { id: 'status', label: 'Battery and status', hint: 'See the battery level' },
  { id: 'sms', label: 'Text messages', hint: 'Read and send SMS of a phone' },
  { id: 'screen', label: 'Phone screen', hint: 'See and control a phone\'s screen' }
];

export type Contact = {
  id: string;
  name: string;
  platform: string;
  user_id: string | null;
  user_name: string | null;
  user_hue: number | null;
  mine: boolean;
  direct: boolean;
  caps: Cap[];
  online: boolean;
  rtt_ms: number | null;
  battery: [number, boolean] | null;
};

export type ShelfItem = {
  id: string;
  name: string;
  size: number;
  owner_user: string;
  owner_user_name: string;
  owner_device: string;
  audience?: unknown;
  created: number;
  mine: boolean;
};
export type UpdateInfo = {
  current: string;
  latest: string;
  notes: string;
  size: number;
  name: string;
  url: string;
  sha256: string | null;
  can_install: boolean;
  page: string;
};
export type SmsThread = { id: string; address: string; name: string | null; snippet: string; date: number; unread: boolean };
export type SmsMessage = { id: string; address: string; body: string; date: number; outgoing: boolean };
export type Settings = { background: boolean; autostart: boolean; allow_remote: boolean; localsend: boolean; quickshare: boolean; clipboard_in: boolean; links_in: boolean; ring_in: boolean; notifications_in: boolean; phone_calls: boolean; sms: boolean; show_shelf: boolean; show_send: boolean; show_activity: boolean };
export type LsPeer = { fingerprint: string; alias: string; model: string | null; device_type: string; ip: string; port: number; https: boolean };
export type LsIncoming = { session: string; from: string; pin?: string; via?: string; files: { id: string; name: string; size: number }[] };
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

export type User = { id: string; name: string; role: 'owner' | 'member'; hue: number | null };
/** Colours people can have, the same on every device. */
export const PALETTE = [8, 28, 46, 98, 150, 176, 200, 222, 252, 282, 312, 340];
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
  | { op: 'set_user_color'; id: string; hue: number }
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
  openExternal: (url: string) => invoke<void>('open_external', { url }),
  appVersion: () => invoke<string>('app_version'),
  checkUpdate: () => invoke<UpdateInfo | null>('check_update'),
  installUpdate: (info: UpdateInfo) => invoke<void>('install_update', { info }),
  smsThreads: (peerId: string) => invoke<SmsThread[]>('sms_threads', { peerId }),
  smsThread: (peerId: string, id: string) => invoke<SmsMessage[]>('sms_thread', { peerId, id }),
  smsSend: (peerId: string, to: string, body: string) => invoke<void>('sms_send', { peerId, to, body }),
  screenOp: (peerId: string, op: ScreenOp) => invoke<unknown>('screen_op', { peerId, op }),
  phoneOp: (peerId: string, op: PhoneOp) => invoke<unknown>('phone_op', { peerId, op }),
  lsPeers: () => invoke<LsPeer[]>('ls_peers'),
  lsSend: (fingerprint: string, paths: string[]) => invoke<void>('ls_send', { fingerprint, paths }),
  lsDecide: (session: string, accept: boolean) => invoke<void>('ls_decide', { session, accept }),
  bluetoothSettings: () => invoke<void>('bluetooth_settings'),
  setMyColor: (hue: number) => invoke<void>('set_my_color', { hue }),
  renameDevice: (name: string) => invoke<string>('rename_device', { name }),
  settings: () => invoke<Settings>('get_settings'),
  setSettings: (next: Settings) => invoke<Settings>('set_settings', { new: next }),
  shelfList: () => invoke<ShelfItem[]>('shelf_list'),
  shelfPut: () => invoke<ShelfItem | null>('shelf_put'),
  shelfPutPath: (path: string) => invoke<ShelfItem>('shelf_put_path', { path }),
  shelfCache: (id: string, name: string, size: number) => invoke<string>('shelf_cache', { id, name, size }),
  dragOut: (id: string, name: string, size: number) => invoke<void>('drag_out', { id, name, size }),
  statPaths: (paths: string[]) => invoke<{ path: string; name: string; size: number; is_file: boolean }[]>('stat_paths', { paths }),
  pickFiles: () => invoke<string[]>('pick_files'),
  sendText: (peerId: string, text: string) => invoke<Delivery>('send_text', { peerId, text }),
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
