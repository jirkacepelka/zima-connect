// This computer's preferences, shared by every screen so a switch in Settings takes effect at once.
import { api, type Settings } from '$lib/api';

export const prefs = $state<{ v: Settings | null }>({ v: null });

export async function loadPrefs() {
  try {
    prefs.v = await api.settings();
  } catch {
    /* not inside the app */
  }
}

/** True unless the person turned that module off (everything is on until the settings are read). */
export const on = (k: keyof Settings) => (prefs.v ? !!prefs.v[k] : true);
