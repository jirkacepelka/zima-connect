import type { Contact } from '$lib/api';

export type Kind = 'phone' | 'tablet' | 'laptop' | 'desktop' | 'server';

/** What the device looks like, guessed from its platform and name. */
export function kindOf(platform: string, name = ''): Kind {
  if (platform === 'server') return 'server';
  if (platform === 'android' || platform === 'ios') return /tab|pad|slate/i.test(name) ? 'tablet' : 'phone';
  // Computers always get the laptop picture, whatever they are called.
  return 'laptop';
}

/** Same hue for the same person everywhere (identity, not data). */
export function hueOf(s: string | null | undefined): number {
  let h = 0;
  for (const c of s ?? '?') h = (h * 31 + c.charCodeAt(0)) % 360;
  return (h * 137) % 360;
}

export const hueFor = (c: Contact) => c.user_hue ?? hueOf(c.user_name ?? c.id);

export const PLATFORM: Record<string, string> = {
  android: 'Android',
  windows: 'Windows',
  linux: 'Linux',
  macos: 'macOS',
  ios: 'iOS',
  server: 'ZimaOS'
};

export const ownerName = (c: Contact) => (c.mine ? 'You' : (c.user_name ?? 'Paired directly'));

export const size = (n: number) =>
  n > 1e9 ? `${(n / 1e9).toFixed(1)} GB` : n > 1e6 ? `${(n / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1e3))} kB`;

export const ext = (name: string) => (name.includes('.') ? name.split('.').pop()!.toLowerCase() : '');
export const isImage = (name: string) => ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'avif'].includes(ext(name));
export const isPdf = (name: string) => ext(name) === 'pdf';
