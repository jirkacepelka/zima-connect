<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '$lib/Icon.svelte';

  type Peer = { id: string; name: string; platform: string; online: boolean; rtt_ms: number | null };
  type Activity = {
    id: number;
    ts: number;
    kind: string;
    title: string;
    detail: string;
    ok: boolean;
    path: string | null;
  };
  type Identity = { id: string; name: string; platform: string };

  let identity = $state<Identity | null>(null);
  let peers = $state<Peer[]>([]);
  let activities = $state<Activity[]>([]);
  let selectedId = $state<string | null>(null);
  let loaded = $state(false);
  let now = $state(Date.now());

  let toast = $state<{ text: string; error: boolean } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout>;
  let busy = $state<string | null>(null);

  // Pairing dialog
  let pairOpen = $state(false);
  let pairTab = $state<'show' | 'enter'>('show');
  let myCode = $state('');
  let codeLoading = $state(false);
  let codeCopied = $state(false);
  let enteredCode = $state('');
  let pairing = $state(false);

  const selected = $derived(peers.find((p) => p.id === selectedId) ?? peers[0] ?? null);

  function say(text: string, error = false) {
    toast = { text, error };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), error ? 5000 : 2600);
  }

  const errText = (e: unknown) => String(e).replace(/^Error:\s*/, '');

  async function refresh() {
    try {
      activities = await invoke<Activity[]>('get_activities');
      peers = await invoke<Peer[]>('get_peers');
      if (!selectedId && peers.length) selectedId = peers[0].id;
    } catch (e) {
      say(errText(e), true);
    } finally {
      loaded = true;
    }
  }

  onMount(() => {
    invoke<Identity>('get_identity').then((i) => (identity = i));
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

  const sendClipboard = () =>
    run('clip', () => invoke('send_clipboard', { peerId: selected!.id }), 'Clipboard sent');
  const sendFile = () =>
    run('file', () => invoke('send_file', { peerId: selected!.id }), 'File sent');
  const ring = () => run('ring', () => invoke('ring_device', { peerId: selected!.id }), 'Device is ringing');

  async function openPairing() {
    pairOpen = true;
    pairTab = 'show';
    enteredCode = '';
    codeCopied = false;
    myCode = '';
    codeLoading = true;
    try {
      myCode = await invoke<string>('create_pairing_code');
    } catch (e) {
      say(errText(e), true);
    } finally {
      codeLoading = false;
    }
  }

  async function copyCode() {
    try {
      await navigator.clipboard.writeText(myCode);
      codeCopied = true;
      setTimeout(() => (codeCopied = false), 1800);
    } catch {
      say('Could not copy. Select the code and copy it manually.', true);
    }
  }

  async function submitPair() {
    if (!enteredCode.trim() || pairing) return;
    pairing = true;
    try {
      const name = await invoke<string>('pair_device', { code: enteredCode.trim() });
      pairOpen = false;
      say(`Paired with ${name}`);
      await refresh();
      const p = peers.find((x) => x.name === name);
      if (p) selectedId = p.id;
    } catch (e) {
      say(errText(e), true);
    } finally {
      pairing = false;
    }
  }

  async function unpair() {
    if (!selected) return;
    const name = selected.name;
    if (!confirm(`Remove “${name}”? You can pair it again later.`)) return;
    try {
      await invoke('unpair_device', { id: selected.id });
      selectedId = null;
      say(`${name} removed`);
      refresh();
    } catch (e) {
      say(errText(e), true);
    }
  }

  // After pairing from the *other* device, this one learns about it through polling.
  // Close the dialog automatically if that happens while the code is shown.
  let knownCount = 0;
  $effect(() => {
    const n = peers.length;
    if (n > knownCount && pairOpen && pairTab === 'show') {
      pairOpen = false;
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
    ({ clipboard: 'clipboard', file: 'file', ring: 'bell', pair: 'link' } as Record<string, string>)[k] ?? 'bell';

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && pairOpen) pairOpen = false;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <aside class="sidebar">
    <div class="brand">
      <div class="logo"><Icon name="link" size={16} /></div>
      <span>Zima Connect</span>
    </div>

    <div class="section-label">My devices</div>
    <nav class="devices">
      {#each peers as p (p.id)}
        <button class="device" class:active={selected?.id === p.id} onclick={() => (selectedId = p.id)}>
          <span class="dev-icon"><Icon name={iconFor(p.platform)} size={18} /></span>
          <span class="dev-text">
            <span class="dev-name">{p.name}</span>
            <span class="dev-sub">{p.online ? 'Connected' : 'Offline'}</span>
          </span>
          <span class="dot" class:on={p.online}></span>
        </button>
      {/each}
      {#if loaded && peers.length === 0}
        <p class="empty-side">No devices yet.</p>
      {/if}
    </nav>

    <button class="add" onclick={openPairing}>
      <Icon name="plus" size={16} /> Add device
    </button>

    <div class="me">
      <div class="me-label">This computer</div>
      <div class="me-name">{identity?.name ?? '…'}</div>
    </div>
  </aside>

  <main class="main">
    {#if !loaded}
      <div class="center"><div class="spinner"></div></div>
    {:else if !selected}
      <div class="center">
        <div class="hero">
          <div class="hero-icon"><Icon name="pair" size={30} /></div>
          <h1>Connect your first device</h1>
          <p>
            Install Zima Connect on another device, then pair the two with a one-time code.
            No account or server needed — devices find each other directly.
          </p>
          <button class="primary big" onclick={openPairing}>Add device</button>
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
          </div>
        </div>
        <button class="ghost" onclick={unpair} title="Remove device"><Icon name="trash" size={16} /> Remove</button>
      </header>

      <section class="actions">
        <button class="action" disabled={!selected.online || !!busy} onclick={sendClipboard}>
          <span class="a-icon"><Icon name="clipboard" size={22} /></span>
          <span class="a-title">Send clipboard</span>
          <span class="a-sub">Share what you copied</span>
        </button>
        <button class="action" disabled={!selected.online || !!busy} onclick={sendFile}>
          <span class="a-icon"><Icon name="file" size={22} /></span>
          <span class="a-title">Send file</span>
          <span class="a-sub">Choose a file to send</span>
        </button>
        <button class="action" disabled={!selected.online || !!busy} onclick={ring}>
          <span class="a-icon"><Icon name="bell" size={22} /></span>
          <span class="a-title">Find device</span>
          <span class="a-sub">Make it ring</span>
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
                  <button class="link" onclick={() => invoke('reveal_path', { path: a.path })}>
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

{#if pairOpen}
  <div class="overlay" role="presentation" onclick={() => (pairOpen = false)}>
    <div class="modal" role="dialog" aria-modal="true" aria-label="Add device" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
      <div class="modal-head">
        <h3>Add device</h3>
        <button class="icon-btn" onclick={() => (pairOpen = false)} aria-label="Close"><Icon name="x" size={16} /></button>
      </div>

      <div class="segmented">
        <button class:on={pairTab === 'show'} onclick={() => (pairTab = 'show')}>Show my code</button>
        <button class:on={pairTab === 'enter'} onclick={() => (pairTab = 'enter')}>Enter a code</button>
      </div>

      {#if pairTab === 'show'}
        <p class="hint">On the other device choose <b>Enter a code</b> and paste this. It works once and expires in 15 minutes.</p>
        <div class="code-box">
          {#if codeLoading}
            <div class="spinner small"></div>
          {:else}
            <code>{myCode}</code>
          {/if}
        </div>
        <div class="modal-actions">
          <button class="primary" onclick={copyCode} disabled={!myCode}>
            <Icon name={codeCopied ? 'check' : 'copy'} size={15} />
            {codeCopied ? 'Copied' : 'Copy code'}
          </button>
        </div>
      {:else}
        <p class="hint">Paste the code shown on the other device.</p>
        <textarea
          class="code-input"
          rows="4"
          placeholder="zc1-…"
          bind:value={enteredCode}
          spellcheck="false"
        ></textarea>
        <div class="modal-actions">
          <button class="primary" onclick={submitPair} disabled={!enteredCode.trim() || pairing}>
            {pairing ? 'Connecting…' : 'Pair'}
          </button>
        </div>
      {/if}
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
</style>
