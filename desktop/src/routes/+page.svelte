<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import HouseholdView from '$lib/HouseholdView.svelte';
  import { api, errText, CAPS, type Activity, type Cap, type Contact, type Household } from '$lib/api';

  let identity = $state<{ id: string; name: string; platform: string } | null>(null);
  let contacts = $state<Contact[]>([]);
  let household = $state<Household | null>(null);
  let activities = $state<Activity[]>([]);
  let selectedId = $state<string | null>(null);
  let view = $state<'device' | 'household'>('device');
  let loaded = $state(false);
  let now = $state(Date.now());

  let toast = $state<{ text: string; error: boolean } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout>;
  let busy = $state<string | null>(null);

  // Add-device dialog
  let dlgOpen = $state(false);
  let tab = $state<'server' | 'show' | 'enter'>('server');
  let serverCode = $state('');
  let yourName = $state('');
  let myCode = $state('');
  let codeLoading = $state(false);
  let codeCopied = $state(false);
  let enteredCode = $state('');
  let working = $state(false);

  // Invitation code dialog (owner creates for a new device / person)
  let invite = $state<{ code: string; label: string } | null>(null);
  let inviteCopied = $state(false);

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
      if (!selectedId && contacts.length) selectedId = contacts[0].id;
    } catch (e) {
      say(errText(e), true);
    } finally {
      loaded = true;
    }
  }

  onMount(() => {
    api.identity().then((i) => (identity = i));
    refresh();
    const poll = setInterval(refresh, 5000);
    const tick = setInterval(() => (now = Date.now()), 30000);
    return () => {
      clearInterval(poll);
      clearInterval(tick);
    };
  });

  async function run(key: string, fn: () => Promise<unknown>, okText: string) {
    if (busy) return;
    busy = key;
    try {
      const r = await fn();
      if (r !== false) say(okText);
    } catch (e) {
      say(errText(e), true);
    } finally {
      busy = null;
      refresh();
    }
  }

  const can = (c: Contact, cap: Cap) => c.online && c.caps.includes(cap);
  const why = (c: Contact, cap: Cap) =>
    !c.caps.includes(cap)
      ? `Not allowed for ${c.user_name ?? 'this device'} by your household rules`
      : !c.online
        ? 'Device is not reachable right now'
        : '';

  const sendClipboard = () => run('clip', () => api.sendClipboard(selected!.id), 'Clipboard sent');
  const sendFile = () => run('file', () => api.sendFile(selected!.id), 'File sent');
  const ring = () => run('ring', () => api.ring(selected!.id), 'Device is ringing');

  async function openDialog(start: 'server' | 'show' | 'enter' = household ? 'show' : 'server') {
    dlgOpen = true;
    serverCode = '';
    enteredCode = '';
    await setTab(start);
  }

  async function setTab(t: 'server' | 'show' | 'enter') {
    tab = t;
    if (t === 'show') {
      myCode = '';
      codeCopied = false;
      codeLoading = true;
      try {
        myCode = await api.pairingCode();
      } catch (e) {
        say(errText(e), true);
      } finally {
        codeLoading = false;
      }
    }
  }

  async function copy(text: string, flag: 'code' | 'invite') {
    try {
      await navigator.clipboard.writeText(text);
      if (flag === 'code') codeCopied = true;
      else inviteCopied = true;
      setTimeout(() => {
        codeCopied = false;
        inviteCopied = false;
      }, 1800);
    } catch {
      say('Could not copy. Select the code and copy it manually.', true);
    }
  }

  async function submitServer() {
    if (!serverCode.trim() || working) return;
    working = true;
    try {
      const name = await api.join(serverCode.trim(), yourName.trim() || null);
      dlgOpen = false;
      say(`Joined ${name}`);
      await refresh();
    } catch (e) {
      say(errText(e), true);
    } finally {
      working = false;
    }
  }

  async function submitPair() {
    if (!enteredCode.trim() || working) return;
    working = true;
    try {
      const name = await api.pair(enteredCode.trim());
      dlgOpen = false;
      say(`Paired with ${name}`);
      await refresh();
      const p = contacts.find((x) => x.name === name);
      if (p) selectedId = p.id;
    } catch (e) {
      say(errText(e), true);
    } finally {
      working = false;
    }
  }

  async function unpair() {
    if (!selected) return;
    const s = selected;
    const msg = s.direct
      ? `Remove “${s.name}”? You can pair it again later.`
      : `Remove “${s.name}” from the household? Only the household owner can do this from the Household page.`;
    if (!s.direct) {
      say('Devices in a household are removed by the owner on the Household page.');
      return;
    }
    if (!confirm(msg)) return;
    try {
      await api.unpair(s.id);
      selectedId = null;
      say(`${s.name} removed`);
      refresh();
    } catch (e) {
      say(errText(e), true);
    }
  }

  // Close the "show my code" dialog automatically when another device pairs with it.
  let knownCount = 0;
  $effect(() => {
    const n = contacts.length;
    if (n > knownCount && dlgOpen && tab === 'show') {
      dlgOpen = false;
      say('Device paired');
    }
    knownCount = n;
  });

  function iconFor(platform: string) {
    if (platform === 'android' || platform === 'ios') return 'phone';
    if (platform === 'server') return 'server';
    if (platform === 'windows' || platform === 'linux' || platform === 'macos') return 'laptop';
    return 'desktop';
  }

  function platformLabel(p: string) {
    const m: Record<string, string> = {
      android: 'Android',
      windows: 'Windows',
      linux: 'Linux',
      macos: 'macOS',
      ios: 'iOS',
      server: 'ZimaOS'
    };
    return m[p] ?? p;
  }

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
    ({ clipboard: 'clipboard', file: 'file', ring: 'bell', notification: 'bell', pair: 'link' } as Record<string, string>)[k] ?? 'bell';

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      dlgOpen = false;
      invite = null;
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <aside class="sidebar">
    <div class="brand">
      <div class="logo"><Icon name="link" size={16} /></div>
      <span>Zima Connect</span>
    </div>

    <nav class="devices">
      {#each groups as g (g.label)}
        <div class="section-label">{g.label}</div>
        {#each g.items as p (p.id)}
          <button
            class="device"
            class:active={view === 'device' && selected?.id === p.id}
            onclick={() => {
              selectedId = p.id;
              view = 'device';
            }}
          >
            <span class="dev-icon"><Icon name={iconFor(p.platform)} size={18} /></span>
            <span class="dev-text">
              <span class="dev-name">{p.name}</span>
              <span class="dev-sub">{p.online ? 'Connected' : 'Offline'}</span>
            </span>
            <span class="dot" class:on={p.online}></span>
          </button>
        {/each}
      {/each}
      {#if loaded && contacts.length === 0}
        <p class="empty-side">No devices yet.</p>
      {/if}
    </nav>

    {#if household}
      <button class="nav-item" class:active={view === 'household'} onclick={() => (view = 'household')}>
        <Icon name="users" size={16} /> Household
      </button>
    {/if}
    <button class="add" onclick={() => openDialog()}>
      <Icon name="plus" size={16} /> Add device
    </button>

    <div class="me">
      <div class="me-label">This computer{household ? ` · ${household.info.me.name}` : ''}</div>
      <div class="me-name">{identity?.name ?? '…'}</div>
    </div>
  </aside>

  <main class="main">
    {#if !loaded}
      <div class="center"><div class="spinner"></div></div>
    {:else if view === 'household' && household}
      <HouseholdView
        {household}
        myDeviceId={identity?.id ?? ''}
        {say}
        onchange={refresh}
        oninvite={(code, label) => {
          invite = { code, label };
        }}
      />
    {:else if !selected}
      <div class="center">
        <div class="hero">
          <div class="hero-icon"><Icon name="pair" size={30} /></div>
          {#if household}
            <h1>You’re in {household.info.household}</h1>
            <p>
              No other devices yet. Add more devices from the Household page, or install Zima Connect on
              another device and join with a code.
            </p>
            <button class="primary big" onclick={() => (view = 'household')}>Open household</button>
          {:else}
            <h1>Connect your devices</h1>
            <p>
              Join your Zima Connect server once and your devices find each other automatically. No account or
              domain needed.
            </p>
            <button class="primary big" onclick={() => openDialog('server')}>Join server</button>
            <button class="textbtn" onclick={() => openDialog('show')}>or pair two devices directly</button>
          {/if}
        </div>
      </div>
    {:else}
      <header class="head">
        <div class="head-icon"><Icon name={iconFor(selected.platform)} size={30} /></div>
        <div class="head-text">
          <h1>{selected.name}</h1>
          <div class="status">
            <span class="dot" class:on={selected.online}></span>
            {#if selected.online}
              Connected{selected.rtt_ms !== null ? ` · ${selected.rtt_ms} ms` : ''}
            {:else}
              Not reachable right now
            {/if}
            <span class="sep">·</span>
            {platformLabel(selected.platform)}
            {#if selected.user_name && !selected.mine}
              <span class="sep">·</span> {selected.user_name}’s device
            {/if}
          </div>
        </div>
        {#if selected.direct}
          <button class="ghost" onclick={unpair} title="Remove device"><Icon name="trash" size={16} /> Remove</button>
        {/if}
      </header>

      <section class="actions">
        <button class="action" disabled={!can(selected, 'clipboard') || !!busy} title={why(selected, 'clipboard')} onclick={sendClipboard}>
          <span class="a-icon"><Icon name="clipboard" size={22} /></span>
          <span class="a-title">Send clipboard</span>
          <span class="a-sub">{selected.caps.includes('clipboard') ? 'Share what you copied' : 'Not allowed'}</span>
        </button>
        <button class="action" disabled={!can(selected, 'files') || !!busy} title={why(selected, 'files')} onclick={sendFile}>
          <span class="a-icon"><Icon name="file" size={22} /></span>
          <span class="a-title">Send file</span>
          <span class="a-sub">{selected.caps.includes('files') ? 'Photos, documents, anything' : 'Not allowed'}</span>
        </button>
        <button class="action" disabled={!can(selected, 'ring') || !!busy} title={why(selected, 'ring')} onclick={ring}>
          <span class="a-icon"><Icon name="bell" size={22} /></span>
          <span class="a-title">Find device</span>
          <span class="a-sub">{selected.caps.includes('ring') ? 'Make it ring' : 'Not allowed'}</span>
        </button>
      </section>

      <section class="activity">
        <h2>Recent activity</h2>
        {#if activities.length === 0}
          <div class="card empty-act">Nothing yet. Things you send and receive will show up here.</div>
        {:else}
          <ul class="card list">
            {#each activities as a (a.id)}
              <li>
                <span class="row-icon" class:bad={!a.ok}><Icon name={a.ok ? activityIcon(a.kind) : 'alert'} size={17} /></span>
                <span class="row-text">
                  <span class="row-title">{a.title}</span>
                  <span class="row-sub">{a.detail}</span>
                </span>
                {#if a.path}
                  <button class="link" onclick={() => api.reveal(a.path!)}>
                    <Icon name="folder" size={14} /> Show
                  </button>
                {/if}
                <span class="row-time">{ago(a.ts)}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}
  </main>
</div>

{#if dlgOpen}
  <div class="overlay" role="presentation" onclick={() => (dlgOpen = false)}>
    <div class="modal" role="dialog" aria-modal="true" aria-label="Add device" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
      <div class="modal-head">
        <h3>Add device</h3>
        <button class="icon-btn" onclick={() => (dlgOpen = false)} aria-label="Close"><Icon name="x" size={16} /></button>
      </div>

      <div class="segmented">
        <button class:on={tab === 'server'} onclick={() => setTab('server')}>Join server</button>
        <button class:on={tab === 'show'} onclick={() => setTab('show')}>Show code</button>
        <button class:on={tab === 'enter'} onclick={() => setTab('enter')}>Enter code</button>
      </div>

      {#if tab === 'server'}
        <p class="hint">
          Open your Zima Connect server’s page (or ask the household owner for an invitation) and paste the code here.
          After this your devices find each other automatically.
        </p>
        <textarea class="code-input" rows="4" placeholder="zch1-…" bind:value={serverCode} spellcheck="false"></textarea>
        <input class="name-input" placeholder="Your name (if you are the first or a new person)" bind:value={yourName} />
        <div class="modal-actions">
          <button class="primary" onclick={submitServer} disabled={!serverCode.trim() || working}>
            {working ? 'Connecting…' : 'Join'}
          </button>
        </div>
      {:else if tab === 'show'}
        <p class="hint">Direct pairing without a server: on the other device choose <b>Enter code</b> and paste this. It works once and expires in 15 minutes.</p>
        <div class="code-box">
          {#if codeLoading}
            <div class="spinner small"></div>
          {:else}
            <code>{myCode}</code>
          {/if}
        </div>
        <div class="modal-actions">
          <button class="primary" onclick={() => copy(myCode, 'code')} disabled={!myCode}>
            <Icon name={codeCopied ? 'check' : 'copy'} size={15} />
            {codeCopied ? 'Copied' : 'Copy code'}
          </button>
        </div>
      {:else}
        <p class="hint">Direct pairing without a server: paste the code shown on the other device.</p>
        <textarea class="code-input" rows="4" placeholder="zc1-…" bind:value={enteredCode} spellcheck="false"></textarea>
        <div class="modal-actions">
          <button class="primary" onclick={submitPair} disabled={!enteredCode.trim() || working}>
            {working ? 'Connecting…' : 'Pair'}
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}

{#if invite}
  <div class="overlay" role="presentation" onclick={() => (invite = null)}>
    <div class="modal" role="dialog" aria-modal="true" aria-label="Invitation" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
      <div class="modal-head">
        <h3>{invite.label}</h3>
        <button class="icon-btn" onclick={() => (invite = null)} aria-label="Close"><Icon name="x" size={16} /></button>
      </div>
      <p class="hint">
        On the new device install Zima Connect, choose <b>Add device → Join server</b> and paste this code.
        It works once and expires in 24 hours.
      </p>
      <div class="code-box"><code>{invite.code}</code></div>
      <div class="modal-actions">
        <button class="primary" onclick={() => copy(invite!.code, 'invite')}>
          <Icon name={inviteCopied ? 'check' : 'copy'} size={15} />
          {inviteCopied ? 'Copied' : 'Copy code'}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if toast}
  <div class="toast" class:error={toast.error} role="status">{toast.text}</div>
{/if}

<style>
  .app {
    display: flex;
    height: 100vh;
  }

  /* ── Sidebar ───────────────────────────── */
  .sidebar {
    width: 260px;
    flex-shrink: 0;
    background: var(--sidebar);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    padding: 18px 12px 14px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 650;
    font-size: 15px;
    padding: 2px 8px 18px;
  }
  .logo {
    width: 26px;
    height: 26px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    display: grid;
    place-items: center;
  }
  .section-label {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-2);
    padding: 0 10px 6px;
  }
  .devices {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .device {
    display: flex;
    align-items: center;
    gap: 11px;
    width: 100%;
    text-align: left;
    padding: 9px 10px;
    border-radius: 10px;
    transition: background 0.12s;
  }
  .device:hover {
    background: var(--hover);
  }
  .device.active {
    background: var(--selected);
  }
  .dev-icon {
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: var(--surface);
    box-shadow: 0 0 0 1px var(--line);
    display: grid;
    place-items: center;
    color: var(--text-2);
    flex-shrink: 0;
  }
  .device.active .dev-icon {
    color: var(--accent);
  }
  .dev-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .dev-name {
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dev-sub {
    font-size: 12px;
    color: var(--text-2);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-3);
    flex-shrink: 0;
    display: inline-block;
  }
  .dot.on {
    background: var(--ok);
  }
  .empty-side {
    color: var(--text-2);
    padding: 6px 10px;
    margin: 0;
    font-size: 13px;
  }
  .add {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    margin: 10px 4px 0;
    padding: 9px 12px;
    border-radius: 10px;
    background: var(--surface);
    box-shadow: 0 0 0 1px var(--line);
    font-weight: 550;
    transition: background 0.12s;
  }
  .add:hover {
    background: var(--surface-2);
  }
  .me {
    margin-top: 14px;
    padding: 12px 10px 0;
    border-top: 1px solid var(--line);
  }
  .me-label {
    font-size: 11.5px;
    color: var(--text-2);
  }
  .me-name {
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Main ──────────────────────────────── */
  .main {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 32px 36px 36px;
  }
  .center {
    height: 100%;
    display: grid;
    place-items: center;
  }
  .hero {
    max-width: 400px;
    text-align: center;
  }
  .hero-icon {
    width: 68px;
    height: 68px;
    border-radius: 20px;
    background: var(--accent-soft);
    color: var(--accent);
    display: grid;
    place-items: center;
    margin: 0 auto 20px;
  }
  .hero h1 {
    margin: 0 0 8px;
    font-size: 24px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .hero p {
    margin: 0 0 24px;
    color: var(--text-2);
    font-size: 14.5px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 26px;
  }
  .head-icon {
    width: 60px;
    height: 60px;
    border-radius: 16px;
    background: var(--surface);
    box-shadow: var(--shadow), 0 0 0 1px var(--line);
    display: grid;
    place-items: center;
    color: var(--accent);
  }
  .head-text {
    flex: 1;
    min-width: 0;
  }
  .head h1 {
    margin: 0;
    font-size: 24px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--text-2);
    margin-top: 2px;
  }
  .sep {
    color: var(--text-3);
  }

  .actions {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 14px;
    margin-bottom: 34px;
  }
  .action {
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow), 0 0 0 1px var(--line);
    padding: 18px;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 2px;
    transition: transform 0.12s, box-shadow 0.12s;
  }
  .action:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: var(--shadow-lg), 0 0 0 1px var(--line);
  }
  .action:active:not(:disabled) {
    transform: translateY(0);
  }
  .action:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .a-icon {
    width: 40px;
    height: 40px;
    border-radius: 11px;
    background: var(--accent-soft);
    color: var(--accent);
    display: grid;
    place-items: center;
    margin-bottom: 12px;
  }
  .a-title {
    font-weight: 600;
    font-size: 14.5px;
  }
  .a-sub {
    color: var(--text-2);
    font-size: 12.5px;
  }

  .activity h2 {
    font-size: 15px;
    font-weight: 650;
    margin: 0 0 12px;
  }
  .card {
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow), 0 0 0 1px var(--line);
  }
  .empty-act {
    padding: 22px;
    color: var(--text-2);
    text-align: center;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 4px 0;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 11px 16px;
  }
  .list li + li {
    border-top: 1px solid var(--line);
  }
  .row-icon {
    width: 32px;
    height: 32px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
    display: grid;
    place-items: center;
    flex-shrink: 0;
  }
  .row-icon.bad {
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    color: var(--warn);
  }
  .row-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .row-title {
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row-sub {
    color: var(--text-2);
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row-time {
    color: var(--text-3);
    font-size: 12px;
    white-space: nowrap;
  }

  /* ── Buttons ───────────────────────────── */
  .primary {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    background: var(--accent);
    color: var(--accent-text);
    padding: 8px 16px;
    border-radius: 9px;
    font-weight: 600;
    transition: filter 0.12s;
  }
  .primary:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .primary:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .primary.big {
    padding: 11px 26px;
    font-size: 15px;
    border-radius: 11px;
  }
  .ghost {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    border-radius: 9px;
    color: var(--text-2);
  }
  .ghost:hover {
    background: var(--hover);
    color: var(--warn);
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--accent);
    font-size: 12.5px;
    font-weight: 550;
    padding: 4px 8px;
    border-radius: 7px;
  }
  .link:hover {
    background: var(--accent-soft);
  }
  .icon-btn {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
  .icon-btn:hover {
    background: var(--hover);
  }

  /* ── Modal ─────────────────────────────── */
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    display: grid;
    place-items: center;
    animation: fade 0.15s ease-out;
  }
  .modal {
    width: 440px;
    max-width: calc(100vw - 40px);
    background: var(--surface);
    border-radius: 18px;
    box-shadow: var(--shadow-lg), 0 0 0 1px var(--line);
    padding: 22px 24px 24px;
    animation: pop 0.18s ease-out;
  }
  .modal-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 16px;
  }
  .modal h3 {
    margin: 0;
    font-size: 18px;
    font-weight: 650;
  }
  .segmented {
    display: flex;
    background: var(--hover);
    border-radius: 10px;
    padding: 3px;
    margin-bottom: 16px;
  }
  .segmented button {
    flex: 1;
    padding: 6px 10px;
    border-radius: 8px;
    font-weight: 550;
    color: var(--text-2);
  }
  .segmented button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12);
  }
  .hint {
    margin: 0 0 12px;
    color: var(--text-2);
    font-size: 13px;
  }
  .code-box {
    min-height: 92px;
    display: grid;
    place-items: center;
    background: var(--bg);
    border-radius: 12px;
    padding: 12px 14px;
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .code-box code {
    font-family: var(--mono);
    font-size: 11.5px;
    word-break: break-all;
    user-select: all;
    max-height: 120px;
    overflow-y: auto;
    color: var(--text);
  }
  .code-input {
    width: 100%;
    resize: none;
    font-family: var(--mono);
    font-size: 11.5px;
    padding: 12px 14px;
    border-radius: 12px;
    border: 0;
    background: var(--bg);
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--line);
    user-select: text;
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 16px;
  }

  /* ── Toast / spinner ───────────────────── */
  .toast {
    position: fixed;
    left: 50%;
    bottom: 26px;
    transform: translateX(-50%);
    background: var(--text);
    color: var(--bg);
    padding: 10px 18px;
    border-radius: 12px;
    font-weight: 550;
    box-shadow: var(--shadow-lg);
    max-width: 80vw;
    animation: rise 0.2s ease-out;
  }
  .toast.error {
    background: var(--warn);
    color: #fff;
  }
  .spinner {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 3px solid var(--line);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  .spinner.small {
    width: 20px;
    height: 20px;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translate(-50%, 8px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    * {
      animation: none !important;
      transition: none !important;
    }
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 9px 12px;
    margin: 6px 4px 0;
    border-radius: 10px;
    font-weight: 550;
    color: var(--text-2);
  }
  .nav-item:hover {
    background: var(--hover);
  }
  .nav-item.active {
    background: var(--selected);
    color: var(--accent);
  }
  .section-label {
    padding-top: 10px;
  }
  .textbtn {
    display: block;
    margin: 14px auto 0;
    color: var(--accent);
    font-weight: 550;
  }
  .textbtn:hover {
    text-decoration: underline;
  }
  .modal .code-input {
    user-select: text;
  }
  .name-input {
    width: 100%;
    margin-top: 10px;
    font: inherit;
    padding: 9px 12px;
    border-radius: 10px;
    border: 0;
    background: var(--bg);
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--line);
    user-select: text;
  }
</style>
