<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import Qr from '$lib/Qr.svelte';
  import Dialog from '$lib/Dialog.svelte';
  import AddDevice from '$lib/AddDevice.svelte';
  import DeviceView from '$lib/DeviceView.svelte';
  import ShelfView from '$lib/ShelfView.svelte';
  import SettingsView from '$lib/SettingsView.svelte';
  import HouseholdView from '$lib/HouseholdView.svelte';
  import { api, errText, type Activity, type Contact, type Household } from '$lib/api';

  type View = 'device' | 'shelf' | 'activity' | 'people' | 'settings';

  let identity = $state<{ id: string; name: string; platform: string } | null>(null);
  let contacts = $state<Contact[]>([]);
  let household = $state<Household | null>(null);
  let activities = $state<Activity[]>([]);
  let selectedId = $state<string | null>(null);
  let view = $state<View>('device');
  let loaded = $state(false);
  let now = $state(Date.now());

  let toast = $state<{ text: string; error: boolean } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout>;

  // Find my device: ringing someone else (until), and being rung (by whom).
  let ringingPeer = $state<{ id: string; until: number } | null>(null);
  let rungBy = $state<string | null>(null);
  let audio: AudioContext | null = null;
  let beepTimer: ReturnType<typeof setInterval> | null = null;

  let addOpen = $state<null | 'server' | 'show' | 'enter'>(null);
  let invite = $state<{ code: string; label: string } | null>(null);
  let inviteCopied = $state(false);
  let leaving = $state(false);

  // Files dropped on the window go to the open device or to the shelf.
  let dragging = $state(false);
  let shelfDrop: ((paths: string[]) => void) | null = null;

  const selected = $derived(contacts.find((c) => c.id === selectedId) ?? contacts[0] ?? null);
  const groups = $derived.by(() => {
    const mine = contacts.filter((c) => c.mine);
    const direct = contacts.filter((c) => c.direct);
    const byUser = new Map<string, Contact[]>();
    for (const c of contacts.filter((c) => !c.mine && !c.direct)) {
      const k = c.user_name ?? 'Others';
      byUser.set(k, [...(byUser.get(k) ?? []), c]);
    }
    const out: { label: string; items: Contact[] }[] = [];
    if (mine.length) out.push({ label: 'My devices', items: mine });
    for (const [k, items] of byUser) out.push({ label: k, items });
    if (direct.length) out.push({ label: 'Paired directly', items: direct });
    return out;
  });

  function say(text: string, error = false) {
    toast = { text, error };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), error ? 5000 : 2600);
  }

  async function refresh() {
    try {
      household = await api.household();
      activities = await api.activities();
      contacts = await api.contacts();
      identity = await api.identity();
      if (!selectedId && contacts.length) selectedId = contacts[0].id;
    } catch (e) {
      say(errText(e), true);
    } finally {
      loaded = true;
    }
  }

  onMount(() => {
    refresh();
    const poll = setInterval(refresh, 5000);
    const tick = setInterval(() => (now = Date.now()), 30000);
    const ringPoll = setInterval(checkRing, 1000);
    let unlisten: (() => void) | null = null;
    import('@tauri-apps/api/webview')
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((e) => {
          const p = e.payload;
          if (p.type === 'enter' || p.type === 'over') dragging = true;
          else if (p.type === 'leave') dragging = false;
          else if (p.type === 'drop') {
            dragging = false;
            dropped(p.paths);
          }
        })
      )
      .then((u) => (unlisten = u))
      .catch(() => {
        /* not running inside the app (browser preview) */
      });
    return () => {
      clearInterval(poll);
      clearInterval(tick);
      clearInterval(ringPoll);
      unlisten?.();
      stopBeeping();
    };
  });

  async function dropped(paths: string[]) {
    if (!paths.length) return;
    if (view === 'shelf' && shelfDrop) return shelfDrop(paths);
    if (view !== 'device' || !selected) return say('Open a device or the shelf, then drop the files.', true);
    const c = selected;
    for (const p of paths) {
      try {
        const d = await api.sendPath(c.id, p);
        say(d === 'queued' ? 'Queued: the server delivers it when the device is back' : `Sent to ${c.name}`);
      } catch (e) {
        say(errText(e), true);
      }
    }
    refresh();
  }

  // -- find my device ---------------------------------------------------------
  const isRinging = (id: string) => !!ringingPeer && ringingPeer.id === id && ringingPeer.until > now;
  async function ring() {
    if (!selected) return;
    try {
      await api.ring(selected.id);
      ringingPeer = { id: selected.id, until: Date.now() + 30000 };
      say('Device is ringing');
    } catch (e) {
      say(errText(e), true);
    }
    refresh();
  }
  async function stopRinging() {
    const id = ringingPeer?.id ?? selected?.id;
    ringingPeer = null;
    if (!id) return;
    try {
      await api.ringStop(id);
      say('Stopped ringing');
    } catch (e) {
      say(errText(e), true);
    }
  }

  async function checkRing() {
    if (ringingPeer && ringingPeer.until < Date.now()) ringingPeer = null;
    try {
      const who = await api.ringState();
      if (who && !rungBy) startBeeping();
      if (!who && rungBy) stopBeeping();
      rungBy = who;
    } catch {
      /* ignore */
    }
  }
  function startBeeping() {
    try {
      audio = audio ?? new AudioContext();
      audio.resume();
      const beep = () => {
        if (!audio) return;
        for (const [t, f] of [[0, 880], [0.25, 1175]] as const) {
          const o = audio.createOscillator();
          const g = audio.createGain();
          o.frequency.value = f;
          g.gain.setValueAtTime(0.0001, audio.currentTime + t);
          g.gain.exponentialRampToValueAtTime(0.4, audio.currentTime + t + 0.02);
          g.gain.exponentialRampToValueAtTime(0.0001, audio.currentTime + t + 0.22);
          o.connect(g).connect(audio.destination);
          o.start(audio.currentTime + t);
          o.stop(audio.currentTime + t + 0.24);
        }
      };
      beep();
      beepTimer = setInterval(beep, 1200);
    } catch {
      /* no audio available */
    }
  }
  function stopBeeping() {
    if (beepTimer) clearInterval(beepTimer);
    beepTimer = null;
  }
  async function silence() {
    stopBeeping();
    rungBy = null;
    await api.stopRing();
  }

  // -- people & pairing ---------------------------------------------------------
  async function unpair() {
    if (!selected?.direct) return;
    const s = selected;
    if (!confirm(`Remove “${s.name}”? You can pair it again later.`)) return;
    try {
      await api.unpair(s.id);
      selectedId = null;
      say(`${s.name} removed`);
      refresh();
    } catch (e) {
      say(errText(e), true);
    }
  }
  async function leave() {
    try {
      await api.leave();
      leaving = false;
      say('You left the server');
      view = 'device';
      refresh();
    } catch (e) {
      say(errText(e), true);
    }
  }
  async function copyInvite() {
    if (!invite) return;
    try {
      await navigator.clipboard.writeText(invite.code);
      inviteCopied = true;
      setTimeout(() => (inviteCopied = false), 1800);
    } catch {
      say('Could not copy. Select the code and copy it manually.', true);
    }
  }

  const icon = (p: string) => (p === 'android' || p === 'ios' ? 'phone' : p === 'server' ? 'server' : 'laptop');
  function ago(ts: number) {
    const s = Math.max(0, Math.round((now - ts) / 1000));
    if (s < 45) return 'just now';
    const m = Math.round(s / 60);
    if (m < 60) return `${m} min ago`;
    const h = Math.round(m / 60);
    if (h < 24) return `${h} h ago`;
    return new Date(ts).toLocaleDateString();
  }
  const activityIcon = (k: string) =>
    ({ clipboard: 'clipboard', file: 'file', ring: 'bell', notification: 'bell', pair: 'link', link: 'globe', shelf: 'shelf' } as Record<string, string>)[k] ?? 'activity';

  const go = (v: View, id?: string) => {
    view = v;
    if (id) selectedId = id;
  };
</script>

<div class="app">
  <aside class="side" aria-label="Navigation">
    <div class="mark">
      <span class="glyph"><Icon name="link" size={15} /></span>
      <div><b>Zima Connect</b><span>{household ? household.info.household : 'Not connected to a server'}</span></div>
    </div>

    <nav class="nav">
      {#each groups as g (g.label)}
        <div class="group">{g.label}</div>
        {#each g.items as p (p.id)}
          <button class="item" aria-current={view === 'device' && selected?.id === p.id ? 'page' : undefined} onclick={() => go('device', p.id)}>
            <Icon name={icon(p.platform)} size={16} />
            <span class="lbl">{p.name}</span>
            <span class="dot" class:on={p.online} title={p.online ? 'Connected' : 'Offline'}></span>
          </button>
        {/each}
      {/each}
      {#if loaded && contacts.length === 0}<p class="empty-side">No other devices yet.</p>{/if}
      <button class="item add" onclick={() => (addOpen = household ? 'show' : 'server')}><Icon name="plus" size={16} /><span class="lbl">Add device</span></button>

      <div class="group">Together</div>
      <button class="item" aria-current={view === 'shelf' ? 'page' : undefined} onclick={() => go('shelf')}><Icon name="shelf" size={16} /><span class="lbl">Shelf</span></button>
      <button class="item" aria-current={view === 'activity' ? 'page' : undefined} onclick={() => go('activity')}><Icon name="activity" size={16} /><span class="lbl">Activity</span></button>
      {#if household}
        <button class="item" aria-current={view === 'people' ? 'page' : undefined} onclick={() => go('people')}><Icon name="users" size={16} /><span class="lbl">People</span></button>
      {/if}
    </nav>

    <button class="me" aria-current={view === 'settings' ? 'page' : undefined} onclick={() => go('settings')}>
      <Icon name="settings" size={16} />
      <span class="who"><b>{identity?.name ?? '…'}</b><span>This computer{household ? ` · ${household.info.me.name}` : ''}</span></span>
    </button>
  </aside>

  <main class="main" class:dragging>
    <div class="content">
      {#if !loaded}
        <div class="center"><div class="spinner"></div></div>
      {:else if view === 'settings'}
        <SettingsView name={identity?.name ?? ''} {household} {say} onRenamed={(n) => identity && (identity = { ...identity, name: n })} onAddDevice={() => (addOpen = household ? 'show' : 'server')} onLeave={() => (leaving = true)} />
      {:else if view === 'shelf'}
        <ShelfView {household} {dragging} {say} onlistenDrop={(h) => (shelfDrop = h)} />
      {:else if view === 'activity'}
        <div class="title-row"><div><h1>Activity</h1><div class="finding">What was sent and received on this computer.</div></div></div>
        <section class="card">
          {#each activities as a (a.id)}
            <div class="row">
              <Icon name={a.ok ? activityIcon(a.kind) : 'alert'} size={18} />
              <div class="grow"><div class="name">{a.title}</div><div class="sub">{a.detail}</div></div>
              {#if a.path}<button class="link" onclick={() => api.reveal(a.path!)}>Show</button>{/if}
              <span class="sub num">{ago(a.ts)}</span>
            </div>
          {:else}
            <div class="empty-line" style="border-top:0">Nothing yet. Things you send and receive show up here.</div>
          {/each}
        </section>
      {:else if view === 'people' && household}
        <HouseholdView {household} myDeviceId={identity?.id ?? ''} {say} onchange={refresh} oninvite={(code, label) => (invite = { code, label })} />
      {:else if selected}
        {#key selected.id}
          <DeviceView c={selected} myName={identity?.name ?? ''} {dragging} ringing={isRinging(selected.id)} {say} {refresh} onUnpair={unpair} onRing={ring} onStopRing={stopRinging} />
        {/key}
      {:else}
        <div class="center">
          <div class="hero">
            <h1>{household ? `You are in ${household.info.household}` : 'Connect your devices'}</h1>
            <p>
              {household
                ? 'No other devices yet. Add a phone or computer: ask the server owner for an invitation, or pair directly.'
                : 'Join your Zima Connect server once and your devices find each other automatically. No account, no domain, no open port.'}
            </p>
            <div class="hero-actions">
              <button class="btn btn-primary" onclick={() => (addOpen = household ? 'show' : 'server')}>{household ? 'Add device' : 'Join server'}</button>
              {#if !household}<button class="btn btn-secondary" onclick={() => (addOpen = 'show')}>Pair directly</button>{/if}
            </div>
          </div>
        </div>
      {/if}
    </div>
  </main>
</div>

{#if addOpen}
  <AddDevice
    start={addOpen}
    pairedCount={contacts.length}
    {say}
    onclose={() => (addOpen = null)}
    ondone={() => {
      addOpen = null;
      refresh();
    }}
  />
{/if}

{#if invite}
  <Dialog title={invite.label} onclose={() => (invite = null)}>
    <p class="lead">On the new device install Zima Connect, choose <b>Add device, Join server</b> and paste or scan this code. It works once and expires in 24 hours.</p>
    <div class="codebox">{invite.code}</div>
    <div class="qr"><Qr text={invite.code} /></div>
    <div class="actions">
      <button class="btn btn-secondary" onclick={() => (invite = null)}>Close</button>
      <button class="btn btn-primary" onclick={copyInvite}><Icon name={inviteCopied ? 'check' : 'copy'} size={15} /> {inviteCopied ? 'Copied' : 'Copy code'}</button>
    </div>
  </Dialog>
{/if}

{#if leaving}
  <Dialog title="Leave the server?" onclose={() => (leaving = false)}>
    <p class="lead">This computer is removed from the household and will no longer see the other devices. You can join again with a new code.</p>
    <div class="actions">
      <button class="btn btn-secondary" onclick={() => (leaving = false)}>Cancel</button>
      <button class="btn btn-danger" onclick={leave}>Leave</button>
    </div>
  </Dialog>
{/if}

{#if rungBy}
  <div class="ring" role="alertdialog" aria-label="Find my device">
    <div class="ring-box">
      <Icon name="bell" size={26} />
      <h3>{rungBy} is looking for this computer</h3>
      <p class="sub">It stops by itself after 30 seconds.</p>
      <button class="btn btn-primary" onclick={silence}>Stop</button>
    </div>
  </div>
{/if}

{#if toast}
  <div class="toast" class:error={toast.error} role="status">{toast.text}</div>
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: 248px 1fr;
    height: 100vh;
  }
  .side {
    display: flex;
    flex-direction: column;
    padding: 20px 12px 12px;
    border-right: 1px solid var(--border);
    background: var(--ground);
    min-height: 0;
  }
  .mark {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px 18px;
  }
  .glyph {
    width: 28px;
    height: 28px;
    border-radius: 7px;
    border: 1.5px solid var(--text);
    display: grid;
    place-items: center;
  }
  .mark b {
    display: block;
    font-weight: 600;
  }
  .mark span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    max-width: 170px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .nav {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }
  .group {
    font-size: 12px;
    color: var(--muted);
    padding: 18px 8px 6px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 32px;
    margin-bottom: 2px;
    padding: 6px 8px;
    border-radius: 6px;
    color: var(--muted);
    text-align: left;
  }
  .item:hover {
    background: var(--raised);
    color: var(--text);
  }
  .item[aria-current='page'] {
    background: var(--active);
    color: var(--text);
  }
  .item .lbl {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item.add {
    margin-top: 4px;
  }
  .empty-side {
    color: var(--muted);
    padding: 4px 8px;
    margin: 0;
  }
  .me {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 8px;
    border-top: 1px solid var(--border);
    margin: 8px -12px 0;
    padding-left: 20px;
    padding-right: 20px;
    text-align: left;
    color: var(--muted);
  }
  .me:hover,
  .me[aria-current='page'] {
    color: var(--text);
    background: var(--raised);
  }
  .who b {
    display: block;
    color: var(--text);
    font-weight: 500;
    max-width: 170px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .who span {
    font-size: 12px;
  }
  .main {
    overflow-y: auto;
    min-width: 0;
    padding: 32px 32px 48px;
    outline: 2px solid transparent;
    outline-offset: -8px;
    transition: outline-color 0.12s;
  }
  .main.dragging {
    outline-color: var(--hue);
  }
  .content {
    max-width: 760px;
    margin: 0 auto;
  }
  .center {
    min-height: 60vh;
    display: grid;
    place-items: center;
  }
  .hero {
    max-width: 420px;
  }
  .hero h1 {
    margin: 0 0 8px;
    font-size: 24px;
    line-height: 32px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .hero p {
    color: var(--muted);
    margin: 0 0 20px;
  }
  .hero-actions {
    display: flex;
    gap: 8px;
  }
  .spinner {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid var(--control);
    border-top-color: var(--text);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .ring {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    display: grid;
    place-items: center;
    z-index: 80;
  }
  .ring-box {
    background: var(--surface);
    border-radius: 12px;
    box-shadow: var(--float);
    padding: 26px 30px;
    text-align: center;
    display: grid;
    gap: 8px;
    justify-items: center;
  }
  .ring-box h3 {
    margin: 4px 0 0;
    font-size: 16px;
  }
  .ring-box p {
    margin: 0 0 8px;
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    background: var(--primary);
    color: var(--primary-ink);
    padding: 10px 16px;
    border-radius: 8px;
    box-shadow: var(--float);
    max-width: 520px;
    z-index: 90;
  }
  .toast.error {
    background: var(--negative);
    color: #fff;
  }
</style>
