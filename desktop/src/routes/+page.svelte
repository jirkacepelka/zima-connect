<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import Qr from '$lib/Qr.svelte';
  import Dialog from '$lib/Dialog.svelte';
  import AddDevice from '$lib/AddDevice.svelte';
  import DeviceView from '$lib/DeviceView.svelte';
  import Home from '$lib/Home.svelte';
  import PhoneOverlay from '$lib/PhoneOverlay.svelte';
  import MessagesView from '$lib/MessagesView.svelte';
  import { wirePhone } from '$lib/phone.svelte';
  import { loadPrefs } from '$lib/prefs.svelte';
  import SettingsView from '$lib/SettingsView.svelte';
  import HouseholdView from '$lib/HouseholdView.svelte';
  import UpdateDialog from '$lib/UpdateDialog.svelte';
  import Logo from '$lib/Logo.svelte';
  import { api, errText, type Activity, type Contact, type Household, type LsIncoming, type LsPeer, type UpdateInfo } from '$lib/api';
  import { size } from '$lib/devices';

  type View = 'home' | 'device' | 'messages' | 'activity' | 'people' | 'settings';

  let identity = $state<{ id: string; name: string; platform: string } | null>(null);
  let contacts = $state<Contact[]>([]);
  let household = $state<Household | null>(null);
  let activities = $state<Activity[]>([]);
  let selectedId = $state<string | null>(null);
  let view = $state<View>('home');
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
  let update = $state<UpdateInfo | null>(null);
  let updateOpen = $state(false);

  async function checkUpdate(manual = false) {
    try {
      update = await api.checkUpdate();
      if (manual) say(update ? `Version ${update.latest} is available` : 'You are up to date');
    } catch (e) {
      if (manual) say(errText(e), true);
    }
  }

  // Files dropped on the window go to the open device or to the shelf.
  let dragging = $state(false);
  let dropHandler: ((paths: string[]) => void) | null = null;
  /** The device card under the cursor while files are dragged over the window. */
  let overId = $state<string | null>(null);

  // LocalSend devices nearby, and files they want to send here.
  let nearby = $state<LsPeer[]>([]);
  let collapsed = $state(false);
  try {
    collapsed = localStorage.getItem('zc-side') === '1';
  } catch {
    /* no storage */
  }
  function toggleSide() {
    collapsed = !collapsed;
    try {
      localStorage.setItem('zc-side', collapsed ? '1' : '0');
    } catch {
      /* no storage */
    }
  }
  let incoming = $state<LsIncoming | null>(null);
  async function decideIncoming(accept: boolean) {
    const i = incoming;
    incoming = null;
    if (!i) return;
    try {
      await api.lsDecide(i.session, accept);
      if (accept) say('Receiving…');
    } catch (e) {
      say(errText(e), true);
    }
  }
  const totalSize = (i: LsIncoming) => size(i.files.reduce((n, f) => n + f.size, 0));

  function deviceAt(pos: { x: number; y: number }): string | null {
    const r = window.devicePixelRatio || 1;
    const el = document.elementFromPoint(pos.x / r, pos.y / r);
    const panel = el?.closest<HTMLElement>('[data-drop-panel]')?.dataset.dropPanel;
    if (panel) return 'panel:' + panel;
    return el?.closest<HTMLElement>('[data-device-id]')?.dataset.deviceId ?? null;
  }

  // Files dropped on the Send or Shelf button of Home: Home opens it and puts them there.
  let homeDrop = $state<{ panel: 'send' | 'shelf'; paths: string[]; n: number } | null>(null);
  let dropCount = 0;

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
      nearby = await api.lsPeers().catch(() => []);
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
    loadPrefs();
    wirePhone().catch(() => {});
    const unlistenLs: (() => void)[] = [];
    import('@tauri-apps/api/event')
      .then(async ({ listen }) => {
        unlistenLs.push(await listen<LsIncoming>('ls-incoming', (e) => (incoming = e.payload)));
        unlistenLs.push(await listen('ls-done', () => refresh()));
      })
      .catch(() => {});
    const firstCheck = setTimeout(checkUpdate, 8000);
    const updatePoll = setInterval(checkUpdate, 6 * 3600 * 1000);
    const poll = setInterval(refresh, 5000);
    const tick = setInterval(() => (now = Date.now()), 30000);
    const ringPoll = setInterval(checkRing, 1000);
    let unlisten: (() => void) | null = null;
    import('@tauri-apps/api/webview')
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((e) => {
          const p = e.payload;
          if (p.type === 'enter' || p.type === 'over') {
            dragging = true;
            overId = view === 'home' ? deviceAt(p.position) : null;
          } else if (p.type === 'leave') {
            dragging = false;
            overId = null;
          } else if (p.type === 'drop') {
            const target = view === 'home' ? deviceAt(p.position) : null;
            dragging = false;
            overId = null;
            dropped(p.paths, target);
          }
        })
      )
      .then((u) => (unlisten = u))
      .catch(() => {
        /* not running inside the app (browser preview) */
      });
    return () => {
      clearTimeout(firstCheck);
      clearInterval(updatePoll);
      clearInterval(poll);
      clearInterval(tick);
      clearInterval(ringPoll);
      unlisten?.();
      unlistenLs.forEach((u) => u());
      stopBeeping();
    };
  });

  async function dropped(paths: string[], target: string | null) {
    if (!paths.length) return;
    if (target?.startsWith('panel:')) {
      homeDrop = { panel: target.slice(6) as 'send' | 'shelf', paths, n: ++dropCount };
      return;
    }
    // Dropped on a device card: send right away.
    if (target) {
      const c = contacts.find((x) => x.id === target);
      if (!c) return;
      for (const p of paths) {
        try {
          const d = await api.sendPath(c.id, p);
          say(d === 'queued' ? `Queued: the server delivers it to ${c.name} when it is back` : `Sent to ${c.name}`);
        } catch (e) {
          say(errText(e), true);
        }
      }
      refresh();
      return;
    }
    if (dropHandler) return dropHandler(paths);
    say('Drop files on the shelf, or open a device first.', true);
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
    ({ clipboard: 'clipboard', file: 'file', ring: 'bell', notification: 'bell', pair: 'link', link: 'globe', shelf: 'shelf', sms: 'clipboard', call: 'call' } as Record<string, string>)[k] ?? 'activity';

  const go = (v: View, id?: string) => {
    view = v;
    if (id) selectedId = id;
  };
</script>

<div class="app" class:collapsed>
  <aside class="side" class:collapsed aria-label="Navigation">
    <div class="mark">
      <span class="glyph"><Logo size={30} /></span>
      <div class="txt"><b>Zima Connect</b><span>{household ? household.info.household : 'Not connected to a server'}</span></div>
    </div>

    <nav class="nav">
      <button class="item" aria-current={view === 'home' ? 'page' : undefined} onclick={() => go('home')}><Icon name="home" size={16} /><span class="lbl">Home</span></button>
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
      {#if household}
        <button class="item" aria-current={view === 'people' ? 'page' : undefined} onclick={() => go('people')}><Icon name="users" size={16} /><span class="lbl">People</span></button>
      {/if}
    </nav>

    <button class="fold" onclick={toggleSide} aria-label={collapsed ? 'Expand the sidebar' : 'Collapse the sidebar'} title={collapsed ? 'Expand' : 'Collapse'}><Icon name="sidebar" size={16} /></button>
    <button class="me" aria-current={view === 'settings' ? 'page' : undefined} onclick={() => go('settings')}>
      <Icon name="settings" size={16} />
      <span class="who"><b>{identity?.name ?? '…'}</b><span>This computer{household ? ` · ${household.info.me.name}` : ''}</span></span>
    </button>
  </aside>

  <main class="main" class:dragging>
    <div class="content">
      {#if update && (view === 'home' || view === 'settings')}
        <div class="banner" role="status">
          <span class="pip"></span>
          <div class="grow">Version <b>{update.latest}</b> is available</div>
          <button class="link" onclick={() => (updateOpen = true)}>What’s new</button>
          <button class="upd" onclick={() => (updateOpen = true)}>Update</button>
        </div>
      {/if}
      {#if !loaded}
        <div class="center"><div class="spinner"></div></div>
      {:else if view === 'settings'}
        <SettingsView name={identity?.name ?? ''} {household} {say} onRenamed={(n) => identity && (identity = { ...identity, name: n })} onAddDevice={() => (addOpen = household ? 'show' : 'server')} onLeave={() => (leaving = true)} onCheckUpdate={() => checkUpdate(true)} />
      {:else if view === 'home'}
        <Home {contacts} {household} {activities} {nearby} {dragging} {overId} {now} drop={homeDrop} {say} onOpenDevice={(id) => go('device', id)} onAdd={() => (addOpen = household ? 'show' : 'server')} onShowActivity={() => go('activity')} onlistenDrop={(h) => (dropHandler = h)} onReveal={(p) => api.reveal(p)} onRefresh={refresh} />
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
      {:else if view === 'messages' && selected}
        <MessagesView c={selected} {say} onBack={() => go('device')} />
      {:else if selected}
        {#key selected.id}
          <DeviceView c={selected} {dragging} ringing={isRinging(selected.id)} {say} {refresh} onUnpair={unpair} onRing={ring} onStopRing={stopRinging} onlistenDrop={(h) => (dropHandler = h)} onMessages={() => go('messages')} onBack={() => go('home')} />
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

{#if incoming}
  <Dialog title="Incoming from a device nearby" onclose={() => decideIncoming(false)}>
    <p class="lead"><b>{incoming.from} wants to send you {incoming.files.length === 1 ? 'a file' : `${incoming.files.length} files`}</b></p>
    <ul class="files">
      {#each incoming.files.slice(0, 6) as f (f.id)}<li>{f.name} <span class="sub num">{size(f.size)}</span></li>{/each}
      {#if incoming.files.length > 6}<li class="sub">and {incoming.files.length - 6} more</li>{/if}
    </ul>
    {#if incoming.pin}<p class="lead">PIN <b class="num">{incoming.pin}</b>: accept only if the other device shows the same.</p>{/if}
    <div class="sub">{totalSize(incoming)} in total. Sent with {incoming.via ?? 'LocalSend'}.</div>
    <div class="actions">
      <button class="btn btn-secondary" onclick={() => decideIncoming(false)}>Decline</button>
      <button class="btn btn-primary" onclick={() => decideIncoming(true)}>Accept</button>
    </div>
  </Dialog>
{/if}

<PhoneOverlay />

{#if updateOpen && update}
  <UpdateDialog info={update} {say} onclose={() => (updateOpen = false)} />
{/if}

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
  .files {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .files li {
    display: flex;
    justify-content: space-between;
    gap: 16px;
  }
  .app {
    display: grid;
    grid-template-columns: 252px 1fr;
    gap: 6px;
    transition: grid-template-columns 0.18s;
    height: 100vh;
    padding: 14px 14px 14px 14px;
    background: var(--ground);
  }
  .side {
    display: flex;
    flex-direction: column;
    padding: 20px 12px 12px;
    border-radius: var(--radius-card);
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: 0 18px 44px -26px rgb(0 0 0 / 0.38);
    overflow: hidden;
    min-height: 0;
  }
  .mark {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px 18px;
  }
  .glyph {
    display: grid;
    flex: none;
  }
  .fold {
    align-self: flex-start;
    margin: 6px 8px 0;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 6px;
    color: var(--muted);
  }
  .fold:hover {
    background: var(--raised);
    color: var(--text);
  }
  .app.collapsed {
    grid-template-columns: 60px 1fr;
  }
  .side.collapsed {
    padding: 14px 7px 7px;
    align-items: center;
  }
  .side.collapsed .txt,
  .side.collapsed .lbl,
  .side.collapsed .group,
  .side.collapsed .empty-side,
  .side.collapsed .who {
    display: none;
  }
  .side.collapsed .mark {
    padding: 0 0 14px;
  }
  .side.collapsed .fold {
    align-self: center;
    margin: 6px 0 0;
  }
  .side.collapsed .nav {
    width: 100%;
  }
  .side.collapsed .item,
  .side.collapsed .me {
    justify-content: center;
    width: 36px;
    height: 36px;
    min-height: 0;
    margin: 0 auto 4px;
    padding: 0;
    border-radius: 8px;
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
    margin: 8px 0 0;
    border-radius: var(--radius-card);
    background: var(--raised);
    padding-left: 12px;
    padding-right: 12px;
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
    padding: 30px 40px 64px;
    outline: 2px solid transparent;
    outline-offset: -8px;
    transition: outline-color 0.12s;
  }
  .main.dragging {
    outline-color: var(--hue);
  }
  .content {
    max-width: 960px;
    margin: 0 auto;
  }
  .center {
    min-height: 60vh;
    display: grid;
    place-items: center;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 7px 8px 7px 14px;
    margin-bottom: 28px;
    background: var(--hue-pale);
    border-radius: 10px;
    font-size: 13px;
  }
  .pip {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--hue);
  }
  .upd {
    height: 26px;
    padding: 0 12px;
    border-radius: 6px;
    background: var(--primary);
    color: var(--primary-ink);
    font-weight: 500;
  }
  .banner .grow {
    flex: 1;
    min-width: 200px;
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
