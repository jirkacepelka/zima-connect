// Phone calls through a phone's SIM. The phone does the call; this window shows who is
// calling and lets you answer, decline, dial and hang up from here.
import { listen } from '@tauri-apps/api/event';
import { api } from '$lib/api';

export type PhoneLine = 'idle' | 'ringing' | 'dialing' | 'talking';

export const phone = $state({
  device: '', // id of the phone
  deviceName: '',
  line: 'idle' as PhoneLine,
  number: '',
  name: null as string | null,
  muted: false,
  since: 0,
  error: ''
});

type Pushed = { from: string; from_name: string; state: string; number: string; name: string | null };

export async function wirePhone() {
  await listen<Pushed>('phone-state', (e) => {
    const p = e.payload;
    const line = (['ringing', 'dialing', 'talking'].includes(p.state) ? p.state : 'idle') as PhoneLine;
    if (line === 'idle') {
      if (phone.device && phone.device !== p.from) return; // another phone's line
      phone.line = 'idle';
      phone.muted = false;
      return;
    }
    if (phone.line !== 'talking' && line === 'talking') phone.since = Date.now();
    phone.device = p.from;
    phone.deviceName = p.from_name;
    phone.line = line;
    phone.number = p.number || phone.number;
    phone.name = p.name ?? (p.number ? null : phone.name);
    phone.error = '';
  });
}

async function run(op: Parameters<typeof api.phoneOp>[1]) {
  phone.error = '';
  try {
    await api.phoneOp(phone.device, op);
  } catch (e) {
    phone.error = typeof e === 'string' ? e : ((e as Error)?.message ?? 'The phone did not answer');
  }
}

export const answer = () => run({ op: 'answer' });
export const decline = async () => {
  await run({ op: 'decline' });
  if (!phone.error) phone.line = 'idle';
};
export const hangUp = async () => {
  await run({ op: 'hang_up' });
  if (!phone.error) phone.line = 'idle';
};
export async function toggleMute() {
  await run({ op: 'mute', on: !phone.muted });
  if (!phone.error) phone.muted = !phone.muted;
}

/** Dial from this computer through the given phone. Throws the phone's error. */
export async function dial(peerId: string, peerName: string, number: string) {
  const n = number.trim();
  if (!n) throw new Error('Type a number first');
  await api.phoneOp(peerId, { op: 'dial', number: n });
  phone.device = peerId;
  phone.deviceName = peerName;
  phone.number = n;
  phone.name = null;
  phone.line = 'dialing';
  phone.error = '';
}
