<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import Mockup from '$lib/Mockup.svelte';
  import RemotePad from '$lib/RemotePad.svelte';
  import { api, errText, type Cap, type Contact } from '$lib/api';
  import { PLATFORM, hueOf, kindOf, ownerName, size } from '$lib/devices';

  let {
    c,
    dragging,
    ringing,
    say,
    refresh,
    onUnpair,
    onRing,
    onStopRing,
    onlistenDrop
  }: {
    c: Contact;
    dragging: boolean;
    ringing: boolean;
    say: (t: string, err?: boolean) => void;
    refresh: () => void;
    onUnpair: () => void;
    onRing: () => void;
    onStopRing: () => void;
    onlistenDrop: (h: ((paths: string[]) => void) | null) => void;
  } = $props();

  // What is about to be sent: files, text and links, all in one place.
  type Staged = { id: number; kind: 'file' | 'text' | 'link'; label: string; detail: string; path?: string; value?: string; state: 'ready' | 'sending' | 'done' | 'failed' };
  let staged = $state<Staged[]>([]);
  let nextId = 1;
  let draft = $state('');
  let sending = $state(false);
  let remoteOpen = $state(false);

  const hue = $derived(hueOf(c.user_name ?? c.id));
  const live: Cap[] = ['ring', 'remote', 'status', 'calls'];
  const can = (cap: Cap) => c.caps.includes(cap) && (c.online || (!c.direct && !live.includes(cap)));
  const why = (cap: Cap) =>
    !c.caps.includes(cap) ? `Not allowed for ${c.user_name ?? 'this device'} by your household rules` : !can(cap) ? 'Device is not reachable right now' : '';
  // Anything can be sent when at least one of files, text or links is allowed.
  const canSend = $derived(can('files') || can('clipboard'));

  onMount(() => {
    onlistenDrop(addPaths);
    return () => onlistenDrop(null);
  });

  async function addPaths(paths: string[]) {
    if (!can('files')) return say(why('files') || 'Sending files is not available', true);
    const infos = await api.statPaths(paths);
    for (const i of infos) {
      if (!i.is_file) {
        say(`“${i.name}” is a folder. Folders cannot be sent yet.`, true);
        continue;
      }
      if (staged.some((s) => s.path === i.path)) continue;
      staged.push({ id: nextId++, kind: 'file', label: i.name, detail: size(i.size), path: i.path, state: 'ready' });
    }
  }
  async function chooseFiles() {
    try {
      const picked = await api.pickFiles();
      if (picked.length) await addPaths(picked);
    } catch (e) {
      say(errText(e), true);
    }
  }

  const URL_RE = /^(https?:\/\/)\S+$/i;
  function addText(raw: string) {
    const t = raw.trim();
    if (!t) return;
    if (URL_RE.test(t) || /^[\w-]+(\.[\w-]+)+(\/\S*)?$/.test(t)) {
      const url = URL_RE.test(t) ? t : `https://${t}`;
      if (!can('clipboard')) return say(why('clipboard'), true);
      staged.push({ id: nextId++, kind: 'link', label: url.replace(/^https?:\/\//, ''), detail: 'opens in the browser', value: url, state: 'ready' });
    } else {
      if (!can('clipboard')) return say(why('clipboard'), true);
      staged.push({ id: nextId++, kind: 'text', label: t.length > 48 ? t.slice(0, 48) + '…' : t, detail: `${t.length} characters`, value: t, state: 'ready' });
    }
    draft = '';
  }
  async function pasteFromClipboard() {
    try {
      addText(await navigator.clipboard.readText());
    } catch {
      say('Could not read the clipboard', true);
    }
  }
  function onPaste(e: ClipboardEvent) {
    const t = e.clipboardData?.getData('text');
    if (t && document.activeElement?.tagName !== 'INPUT') {
      e.preventDefault();
      addText(t);
    }
  }

  const remove = (id: number) => (staged = staged.filter((s) => s.id !== id));

  async function sendAll() {
    if (sending) return;
    sending = true;
    let queued = 0;
    let ok = 0;
    for (const s of staged.filter((x) => x.state === 'ready' || x.state === 'failed')) {
      s.state = 'sending';
      try {
        if (s.kind === 'file') {
          if ((await api.sendPath(c.id, s.path!)) === 'queued') queued++;
        } else if (s.kind === 'text') {
          if ((await api.sendText(c.id, s.value!)) === 'queued') queued++;
        } else await api.sendUrl(c.id, s.value!);
        s.state = 'done';
        ok++;
      } catch (e) {
        s.state = 'failed';
        s.detail = errText(e);
      }
    }
    sending = false;
    const failed = staged.filter((s) => s.state === 'failed').length;
    if (ok) say(queued ? `Sent. ${queued} will arrive when ${c.name} is back (the server keeps it)` : `Sent to ${c.name}`, false);
    if (failed) say(`${failed} could not be sent`, true);
    setTimeout(() => (staged = staged.filter((s) => s.state !== 'done')), 900);
    refresh();
  }

  const readyCount = $derived(staged.filter((s) => s.state === 'ready' || s.state === 'failed').length);
</script>

<svelte:window onpaste={onPaste} />

<div class="layout" style="--h:{hue}">
<div class="hero">
  <div class="art"><Mockup kind={kindOf(c.platform, c.name)} platform={c.platform} {hue} online={c.online} battery={c.battery} size={250} /></div>
  <div class="meta">
    <h1>{c.name}</h1>
    <div class="line num"><span class="dot" class:on={c.online}></span>{c.online ? `Connected${c.rtt_ms !== null ? `, ${c.rtt_ms} ms` : ''}` : 'Not reachable right now'}</div>
    <div class="line"><span class="av">{(c.user_name ?? '·').slice(0, 1).toUpperCase()}</span>{ownerName(c)} · {PLATFORM[c.platform] ?? c.platform}</div>
    {#if c.battery}
      <div class="battery" class:charging={c.battery[1]} aria-label="Battery {c.battery[0]} percent{c.battery[1] ? ', charging' : ''}">
        <div class="gauge"><i style="width:{c.battery[0]}%"></i></div>
        <span class="num">{c.battery[0]} %{c.battery[1] ? ' · charging' : ''}</span>
      </div>
    {/if}
    {#if c.direct}<button class="link-danger rm" onclick={onUnpair}>Remove this device</button>{/if}
  </div>
</div>

<section class="send" class:over={dragging && canSend} class:off={!canSend} aria-labelledby="h-send">
  <h2 id="h-send" class="sr">Send</h2>
  {#if !canSend}
    <div class="zone"><Icon name="alert" size={20} /><b>Sending is not available</b><span class="sub">{why('files') || why('clipboard')}</span></div>
  {:else if staged.length === 0}
    <div class="zone" class:over={dragging}>
      <Icon name="upload" size={26} />
      <b>Drop files here to send them to {c.name}</b>
      <span class="sub">or paste text or a link below. Nothing is sent until you press Send.</span>
      <div class="row-btns">
        <button class="btn btn-secondary" disabled={!can('files')} onclick={chooseFiles}>Choose files…</button>
        <button class="btn btn-secondary" disabled={!can('clipboard')} onclick={pasteFromClipboard}>Paste from clipboard</button>
      </div>
    </div>
  {:else}
    <ul class="staged" aria-label="Ready to send">
      {#each staged as s (s.id)}
        <li class="chip-row {s.state}">
          <span class="ico"><Icon name={s.kind === 'file' ? 'file' : s.kind === 'link' ? 'globe' : 'clipboard'} size={16} /></span>
          <span class="lbl"><b>{s.label}</b><span class="sub num">{s.detail}</span></span>
          {#if s.state === 'sending'}<span class="spin" aria-label="Sending"></span>
          {:else if s.state === 'done'}<span class="okm"><Icon name="check" size={16} /></span>
          {:else}<button class="x" onclick={() => remove(s.id)} aria-label="Remove {s.label}"><Icon name="x" size={14} /></button>{/if}
        </li>
      {/each}
    </ul>
    <div class="more sub">Drop more files anywhere here{can('files') ? '' : ''}.</div>
  {/if}

  {#if canSend}
    <form
      class="bar"
      onsubmit={(e) => {
        e.preventDefault();
        if (draft.trim()) addText(draft);
        else if (readyCount) sendAll();
      }}
    >
      <input class="field" bind:value={draft} placeholder="Type text or paste a link" aria-label="Text or link to send" disabled={!can('clipboard')} autocomplete="off" />
      {#if staged.length > 0}
        <button type="button" class="btn btn-secondary" onclick={chooseFiles} disabled={!can('files')}>Add files…</button>
      {/if}
      <button class="btn btn-primary" type="button" disabled={sending || (!readyCount && !draft.trim())} onclick={() => (draft.trim() ? addText(draft) : sendAll())}>
        {draft.trim() ? 'Add' : sending ? 'Sending…' : readyCount ? `Send ${readyCount}` : 'Send'}
      </button>
    </form>
  {/if}
</section>

<section class="tools" aria-labelledby="h-tools">
  <h2 id="h-tools" class="sect">Tools</h2>
  <div class="row slim">
    <Icon name="bell" size={18} />
    <div class="grow"><div class="name">Find device</div><div class="sub">{can('ring') ? (ringing ? 'Ringing. Stops by itself after 30 s.' : 'Make it ring, even when silent') : why('ring')}</div></div>
    {#if ringing}
      <button class="btn btn-primary" onclick={onStopRing}>Stop</button>
    {:else}
      <button class="btn btn-secondary" disabled={!can('ring')} onclick={onRing}>Ring</button>
    {/if}
  </div>
  <div class="row slim">
    <Icon name="touchpad" size={18} />
    <div class="grow"><div class="name">Remote control</div><div class="sub">{can('remote') ? 'Touchpad, keyboard and media keys' : why('remote')}</div></div>
    <button class="btn btn-secondary" disabled={!can('remote')} aria-expanded={remoteOpen} onclick={() => (remoteOpen = !remoteOpen)}>{remoteOpen ? 'Hide' : 'Open'}</button>
  </div>
  {#if remoteOpen && can('remote')}<RemotePad peerId={c.id} name={c.name} {say} />{/if}
</section>
</div>

<style>
  /* The device on the left, what you do with it on the right. */
  .layout {
    display: grid;
    grid-template-columns: 280px minmax(0, 1fr);
    grid-template-areas: 'hero send' 'tools send';
    grid-template-rows: auto 1fr;
    column-gap: 44px;
    align-items: start;
  }
  .hero {
    grid-area: hero;
    display: flex;
    flex-direction: column;
    gap: 18px;
    margin-bottom: 28px;
  }
  .art {
    display: grid;
    place-items: center;
    padding: 26px 0 18px;
    border-radius: 20px;
    background: hsl(var(--h) 40% 95%);
  }
  .send {
    grid-area: send;
  }
  .tools {
    grid-area: tools;
  }
  @media (max-width: 1000px) {
    .layout {
      grid-template-columns: 1fr;
      grid-template-areas: 'hero' 'send' 'tools';
    }
  }
  @media (prefers-color-scheme: dark) {
    .art {
      background: hsl(var(--h) 22% 11%);
    }
  }
  .meta h1 {
    margin: 0 0 6px;
    font-size: 24px;
    line-height: 30px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    margin-bottom: 2px;
  }
  .av {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: hsl(var(--h) 55% 90%);
    color: hsl(var(--h) 55% 28%);
    display: inline-grid;
    place-items: center;
    font-size: 11px;
    font-weight: 600;
  }
  @media (prefers-color-scheme: dark) {
    .av {
      background: hsl(var(--h) 35% 24%);
      color: hsl(var(--h) 70% 85%);
    }
  }
  .battery {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 10px;
  }
  .battery .gauge {
    width: 120px;
    height: 8px;
    border-radius: 99px;
    background: var(--active);
    overflow: hidden;
  }
  .battery .gauge i {
    display: block;
    height: 100%;
    background: var(--on);
    border-radius: 99px;
    transition: width 0.6s ease;
  }
  .battery.charging .gauge i {
    background: linear-gradient(90deg, var(--on) 0%, color-mix(in srgb, var(--on) 55%, white) 50%, var(--on) 100%);
    background-size: 200% 100%;
    animation: flow 1.6s linear infinite;
  }
  @keyframes flow {
    to {
      background-position: -200% 0;
    }
  }
  .rm {
    margin-top: 8px;
    font-size: 13px;
  }

  .send {
    margin-bottom: 40px;
    border-radius: 16px;
    transition: box-shadow 0.12s;
  }
  .send.over {
    box-shadow: 0 0 0 2px var(--hue);
  }
  .zone {
    margin: 0;
    padding: 84px 24px;
    display: grid;
    justify-items: center;
    gap: 6px;
    text-align: center;
    border: 1.5px dashed var(--control);
    border-radius: 16px;
    color: var(--muted);
    transition: background 0.12s, border-color 0.12s;
  }
  .zone b {
    color: var(--text);
    font-size: 15px;
  }
  .zone.over,
  .send.over .zone {
    border-color: var(--hue);
    background: var(--hue-pale);
  }
  .row-btns {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  .staged {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .chip-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 12px;
    border: 0;
    border-radius: 10px;
    background: var(--raised);
    animation: lock 0.22s ease-out;
  }
  @keyframes lock {
    from {
      transform: scale(1.04);
      opacity: 0;
    }
    to {
      transform: none;
      opacity: 1;
    }
  }
  .chip-row.done {
    box-shadow: inset 0 0 0 1.5px var(--on);
  }
  .chip-row.failed {
    box-shadow: inset 0 0 0 1.5px var(--negative);
  }
  .ico {
    color: var(--muted);
    display: grid;
  }
  .lbl {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .lbl b {
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .x {
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: 6px;
    color: var(--muted);
  }
  .x:hover {
    background: var(--active);
    color: var(--text);
  }
  .okm {
    color: var(--on);
  }
  .spin {
    width: 16px;
    height: 16px;
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
  .more {
    padding: 10px 2px 0;
  }
  .bar {
    display: flex;
    gap: 8px;
    padding: 16px 0 0;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .sect {
    margin: 0 0 8px;
    font-size: 16px;
    font-weight: 600;
  }
  .tools {
    margin-bottom: 16px;
  }
  .row.slim {
    min-height: 56px;
    padding: 8px 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .battery.charging .gauge i,
    .chip-row,
    .spin {
      animation: none;
    }
  }
</style>
