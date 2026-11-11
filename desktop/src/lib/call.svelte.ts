// Voice calls in the window: the microphone and the speaker live here, the transport in the app.
// Audio on the wire is PCM16 mono 16 kHz in 20 ms frames, base64 over the Tauri bridge.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type CallState = 'idle' | 'incoming' | 'ringing' | 'connected' | 'ended';

export const call = $state({
  id: 0,
  peer: '',
  state: 'idle' as CallState,
  outgoing: false,
  muted: false,
  since: 0,
  note: ''
});

const WORKLET = `
class Capture extends AudioWorkletProcessor {
  constructor() { super(); this.buf = new Int16Array(320); this.n = 0; }
  process(inputs) {
    const ch = inputs[0] && inputs[0][0];
    if (!ch) return true;
    for (let i = 0; i < ch.length; i++) {
      const v = Math.max(-1, Math.min(1, ch[i]));
      this.buf[this.n++] = v < 0 ? v * 0x8000 : v * 0x7fff;
      if (this.n === 320) { this.port.postMessage(this.buf.buffer.slice(0)); this.n = 0; }
    }
    return true;
  }
}
registerProcessor('zc-capture', Capture);
class Player extends AudioWorkletProcessor {
  constructor() {
    super(); this.q = []; this.off = 0; this.primed = false;
    this.port.onmessage = (e) => { this.q.push(new Int16Array(e.data)); if (this.q.length > 14) this.q.splice(0, this.q.length - 5); };
  }
  process(_, outputs) {
    const out = outputs[0][0];
    if (!this.primed) { if (this.q.length >= 3) this.primed = true; else { out.fill(0); return true; } }
    let i = 0;
    while (i < out.length) {
      if (!this.q.length) { this.primed = false; out.fill(0, i); break; }
      const a = this.q[0];
      const take = Math.min(a.length - this.off, out.length - i);
      for (let k = 0; k < take; k++) out[i + k] = a[this.off + k] / 32768;
      this.off += take; i += take;
      if (this.off >= a.length) { this.q.shift(); this.off = 0; }
    }
    return true;
  }
}
registerProcessor('zc-player', Player);
`;

let ctx: AudioContext | null = null;
let mic: MediaStream | null = null;
let player: AudioWorkletNode | null = null;
let ringTimer: ReturnType<typeof setInterval> | null = null;
let resetTimer: ReturnType<typeof setTimeout> | null = null;

const b64 = (buf: ArrayBuffer) => {
  let s = '';
  const b = new Uint8Array(buf);
  for (let i = 0; i < b.length; i++) s += String.fromCharCode(b[i]);
  return btoa(s);
};
const unb64 = (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0)).buffer;

async function startAudio(id: number) {
  ctx = new AudioContext({ sampleRate: 16000, latencyHint: 'interactive' });
  await ctx.audioWorklet.addModule(URL.createObjectURL(new Blob([WORKLET], { type: 'application/javascript' })));
  mic = await navigator.mediaDevices.getUserMedia({
    audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 }
  });
  const src = ctx.createMediaStreamSource(mic);
  const cap = new AudioWorkletNode(ctx, 'zc-capture');
  cap.port.onmessage = (e) => {
    if (call.muted || call.state !== 'connected') return;
    invoke('call_send', { callId: id, data: b64(e.data as ArrayBuffer) }).catch(() => {});
  };
  src.connect(cap);
  player = new AudioWorkletNode(ctx, 'zc-player', { outputChannelCount: [1] });
  player.connect(ctx.destination);
  await ctx.resume();
}

function stopAudio() {
  mic?.getTracks().forEach((t) => t.stop());
  mic = null;
  player = null;
  ctx?.close().catch(() => {});
  ctx = null;
}

/** A two-tone ring while somebody is calling. */
function startRing() {
  stopRing();
  try {
    const c = new AudioContext();
    const beep = () => {
      for (const [t, f] of [[0, 523], [0.28, 659]] as const) {
        const o = c.createOscillator();
        const g = c.createGain();
        o.frequency.value = f;
        g.gain.setValueAtTime(0.0001, c.currentTime + t);
        g.gain.exponentialRampToValueAtTime(0.25, c.currentTime + t + 0.03);
        g.gain.exponentialRampToValueAtTime(0.0001, c.currentTime + t + 0.25);
        o.connect(g).connect(c.destination);
        o.start(c.currentTime + t);
        o.stop(c.currentTime + t + 0.27);
      }
    };
    beep();
    ringTimer = setInterval(beep, 1800);
    (startRing as unknown as { ctx?: AudioContext }).ctx = c;
  } catch {
    /* no audio available */
  }
}
function stopRing() {
  if (ringTimer) clearInterval(ringTimer);
  ringTimer = null;
  const c = (startRing as unknown as { ctx?: AudioContext }).ctx;
  c?.close().catch(() => {});
  (startRing as unknown as { ctx?: AudioContext }).ctx = undefined;
}

function finish(note: string) {
  stopRing();
  stopAudio();
  call.state = 'ended';
  call.note = note;
  if (resetTimer) clearTimeout(resetTimer);
  resetTimer = setTimeout(() => {
    call.state = 'idle';
    call.id = 0;
    call.muted = false;
  }, 2600);
}

let wired = false;
/** Start listening to the app's call events (once, from the page). */
export async function wireCalls() {
  if (wired) return;
  wired = true;
  await listen<{ kind: string; call_id: number; peer: string; state?: string }>('call-event', async (e) => {
    const p = e.payload;
    if (p.kind === 'incoming') {
      if (resetTimer) clearTimeout(resetTimer);
      Object.assign(call, { id: p.call_id, peer: p.peer, state: 'incoming', outgoing: false, muted: false, note: '' });
      startRing();
      return;
    }
    if (p.call_id !== call.id && call.state !== 'idle' && call.state !== 'ended') return;
    switch (p.state) {
      case 'ringing':
        Object.assign(call, { id: p.call_id, peer: p.peer, state: 'ringing', outgoing: true });
        break;
      case 'connected':
        stopRing();
        try {
          await startAudio(p.call_id);
          Object.assign(call, { state: 'connected', since: Date.now() });
        } catch (err) {
          invoke('call_hangup', { callId: p.call_id }).catch(() => {});
          const name = (err as { name?: string })?.name ?? '';
          finish(name === 'NotAllowedError' ? 'Allow the microphone for Zima Connect, then call again' : name === 'NotFoundError' ? 'No microphone found' : `Audio problem: ${String(err)}`);
        }
        break;
      case 'declined':
        finish(call.outgoing ? `${p.peer} declined` : 'Call declined');
        break;
      case 'busy':
        finish(`${p.peer} is busy`);
        break;
      case 'unreachable':
        finish(`Could not reach ${p.peer}`);
        break;
      case 'ended':
        finish('Call ended');
        break;
    }
  });
  await listen<[number, string]>('call-audio', (e) => {
    const [id, data] = e.payload;
    if (id === call.id && player) player.port.postMessage(unb64(data));
  });
}

export async function startCall(peerId: string, name: string) {
  if (call.state !== 'idle') throw new Error('You are already in a call');
  const id = await invoke<number>('call_start', { peerId });
  Object.assign(call, { id, peer: name, state: 'ringing', outgoing: true, muted: false, note: '' });
}
export const acceptCall = () => invoke('call_accept', { callId: call.id }).then(() => stopRing());
export const declineCall = () => {
  stopRing();
  return invoke('call_decline', { callId: call.id });
};
export const hangUp = () => invoke('call_hangup', { callId: call.id });
export const toggleMute = () => (call.muted = !call.muted);
