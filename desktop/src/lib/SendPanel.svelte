<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import Dialog from '$lib/Dialog.svelte';
  import { api, errText, type Contact, type LsPeer } from '$lib/api';
  import Thumb from '$lib/Thumb.svelte';
  import Mockup from '$lib/Mockup.svelte';
  import { hueFor, kindOf, size } from '$lib/devices';

  let {
    contacts,
    nearby,
    dragging,
    say,
    refresh,
    onlistenDrop,
    onclose
  }: {
    contacts: Contact[];
    nearby: LsPeer[];
    dragging: boolean;
    say: (t: string, err?: boolean) => void;
    refresh: () => void;
    onlistenDrop: (h: ((paths: string[]) => void) | null) => void;
    onclose: () => void;
  } = $props();

  type Staged = { id: number; kind: 'file' | 'text' | 'link'; label: string; detail: string; path?: string; bytes?: number; value?: string; state: 'ready' | 'sending' | 'done' | 'failed' };
  let staged = $state<Staged[]>([]);
  let nextId = 1;
  let sending = $state(false);
  let picking = $state(false);
  let target = $state<string | null>(null); // a device id, or 'ls:' + fingerprint

  const reachable = (c: Contact) => c.online || !c.direct; // the server keeps things for offline devices
  const mine = $derived(contacts.filter((c) => c.mine));
  const others = $derived(contacts.filter((c) => !c.mine));
  const dest = $derived(
    target?.startsWith('ls:')
      ? { kind: 'ls' as const, peer: nearby.find((p) => 'ls:' + p.fingerprint === target) ?? null }
      : { kind: 'device' as const, contact: contacts.find((c) => c.id === target) ?? null }
  );
  const destName = $derived(dest.kind === 'ls' ? dest.peer?.alias : dest.contact?.name);
  const readyCount = $derived(staged.filter((s) => s.state === 'ready' || s.state === 'failed').length);

  // What you copied appears as something to send, once.
  let seenClip = '';
  async function peekClipboard() {
    try {
      const t = (await api.clipboardText()).trim();
      if (t && t !== seenClip) {
        seenClip = t;
        addText(t);
      }
    } catch {
      /* nothing readable */
    }
  }

  onMount(() => {
    onlistenDrop(addPaths);
    peekClipboard();
    window.addEventListener('focus', peekClipboard);
    return () => {
      onlistenDrop(null);
      window.removeEventListener('focus', peekClipboard);
    };
  });

  // Keep the choice valid when devices come and go.
  $effect(() => {
    if (target && !(dest.kind === 'ls' ? dest.peer : dest.contact)) target = null;
  });

  async function addPaths(paths: string[]) {
    const infos = await api.statPaths(paths);
    for (const i of infos) {
      if (!i.is_file) {
        say(`“${i.name}” is a folder. Folders cannot be sent yet.`, true);
        continue;
      }
      if (staged.some((s) => s.path === i.path)) continue;
      staged.push({ id: nextId++, kind: 'file', label: i.name, detail: size(i.size), path: i.path, bytes: i.size, state: 'ready' });
    }
  }
  async function choose() {
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
    if (URL_RE.test(t)) staged.push({ id: nextId++, kind: 'link', label: t.replace(/^https?:\/\//, ''), detail: 'opens in the browser', value: t, state: 'ready' });
    else staged.push({ id: nextId++, kind: 'text', label: t.length > 48 ? t.slice(0, 48) + '…' : t, detail: `${t.length} characters`, value: t, state: 'ready' });
  }
  function choosePeer(id: string) {
    target = id;
    picking = false;
    sendAll();
  }
  const remove = (id: number) => (staged = staged.filter((s) => s.id !== id));

  async function sendAll() {
    if (sending || !target) return;
    const items = staged.filter((x) => x.state === 'ready' || x.state === 'failed');
    sending = true;
    try {
      if (dest.kind === 'ls') {
        const peer = dest.peer;
        if (!peer) return;
        if (items.some((i) => i.kind !== 'file')) {
          say('LocalSend devices only take files. Remove the text or link, or choose another device.', true);
          return;
        }
        items.forEach((i) => (i.state = 'sending'));
        say(`Waiting for ${peer.alias} to accept…`);
        try {
          await api.lsSend(peer.fingerprint, items.map((i) => i.path!));
          items.forEach((i) => (i.state = 'done'));
          say(`Sent to ${peer.alias}`);
        } catch (e) {
          items.forEach((i) => (i.state = 'failed'));
          say(errText(e), true);
        }
      } else {
        const c = dest.contact;
        if (!c) return;
        let queued = 0;
        let ok = 0;
        for (const s of items) {
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
        const failed = items.filter((s) => s.state === 'failed').length;
        if (ok) say(queued ? `Sent. ${queued} will arrive when ${c.name} is back` : `Sent to ${c.name}`);
        if (failed) say(`${failed} could not be sent`, true);
      }
    } finally {
      sending = false;
      setTimeout(() => (staged = staged.filter((s) => s.state !== 'done')), 900);
      refresh();
    }
  }
</script>

<Dialog wide title={picking ? 'Send to' : 'Send'} onback={picking ? () => (picking = false) : undefined} {onclose}>
  {#if !picking}
    <section class="droppable" class:over={dragging && staged.length > 0} aria-label="Send">
      <div class="panel-head">
        <span class="meta">Goes straight to the device you pick · nothing is kept</span>
        <button class="btn btn-primary" onclick={choose}><Icon name="plus" size={15} /> Add files…</button>
      </div>
      {#if staged.length === 0}
        <div class="dropzone" class:over={dragging}>
          <Icon name="upload" size={22} />
          <b>Drop files here</b>
          <span class="sub">Text or a link you copied shows up here by itself. You pick who gets it next.</span>
        </div>
      {:else}
        <div class="tile-grid" role="list" aria-label="Ready to send">
          {#each staged as s (s.id)}
            <div class="tile {s.state}" role="listitem">
              {#if s.kind === 'file'}
                <Thumb item={{ name: s.label, size: s.bytes ?? 0 }} path={s.path} />
              {:else}
                <div class="txt"><Icon name={s.kind === 'link' ? 'globe' : 'clipboard'} size={26} /><span>{s.kind === 'link' ? 'link' : 'text'}</span></div>
              {/if}
              <div class="cap">
                <div class="n" title={s.label}>{s.label}</div>
                <div class="sub num">{s.detail}</div>
              </div>
              {#if s.state === 'sending'}<span class="st"><span class="spin" aria-label="Sending"></span></span>
              {:else if s.state === 'done'}<span class="st ok"><Icon name="check" size={20} /></span>
              {:else}<div class="acts"><button class="mini" onclick={() => remove(s.id)} aria-label="Remove {s.label}" title="Remove"><Icon name="x" size={14} /></button></div>{/if}
            </div>
          {/each}
        </div>
        <div class="panel-foot">
          <span class="sub">Drop files here to add them.</span>
          <button type="button" class="btn btn-primary go" disabled={sending || !readyCount} onclick={() => (picking = true)}>{sending ? 'Sending…' : `Send ${readyCount}…`}</button>
        </div>
      {/if}
    </section>
    <div class="actions"><button class="btn btn-secondary" onclick={onclose}>Close</button></div>
  {:else}
    <p class="lead"><b>Who gets {readyCount === 1 ? 'it' : `these ${readyCount} items`}?</b></p>
    {#if contacts.length === 0 && nearby.length === 0}
      <div class="sub">No devices yet. Add one, or open LocalSend or Quick Share on a device nearby.</div>
    {/if}
    {#each [{ label: 'My devices', items: mine }, { label: 'Others in the household', items: others }] as g}
      {#if g.items.length}
        <h4>{g.label}</h4>
        <div class="picks">
          {#each g.items as c (c.id)}
            <button class="pick" class:off={!reachable(c)} disabled={!reachable(c)} style="--h:{hueFor(c)}" onclick={() => choosePeer(c.id)}>
              <Mockup kind={kindOf(c.platform, c.name)} platform={c.platform} hue={hueFor(c)} online={c.online} battery={c.battery} size={84} fit="height" maxWidth={96} />
              <b>{c.name}</b>
              <span class="sub"><i class="dot" class:live={c.online}></i>{c.online ? 'Connected' : reachable(c) ? 'Via the server' : 'Offline'}</span>
            </button>
          {/each}
        </div>
      {/if}
    {/each}
    {#if nearby.length}
      <h4>Nearby</h4>
      <div class="picks">
        {#each nearby as p (p.fingerprint)}
          <button class="pick" onclick={() => choosePeer('ls:' + p.fingerprint)}>
            <Mockup kind={p.device_type === 'mobile' ? 'phone' : 'laptop'} platform="" hue={210} online={true} size={84} fit="height" maxWidth={96} />
            <b>{p.alias}</b>
            <span class="sub">{p.fingerprint.startsWith('qs:') ? 'Quick Share' : 'LocalSend'}</span>
          </button>
        {/each}
      </div>
    {/if}
  {/if}
</Dialog>

<style>
  .tile .txt {
    aspect-ratio: 4 / 3;
    border-radius: var(--radius-card);
    background: var(--raised);
    color: var(--muted);
    display: grid;
    place-items: center;
    align-content: center;
    gap: 6px;
  }
  .tile .txt span {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 600;
  }
  .tile.failed :global(.thumb),
  .tile.failed .txt {
    box-shadow: inset 0 0 0 1.5px var(--negative);
  }
  .st {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    aspect-ratio: 4 / 3;
    display: grid;
    place-items: center;
    border-radius: var(--radius-card);
    background: color-mix(in srgb, var(--surface) 70%, transparent);
  }
  .st.ok {
    color: var(--on);
  }
  .spin {
    width: 20px;
    height: 20px;
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
  .go {
    margin-left: auto;
  }
  h4 {
    margin: 16px 0 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }
  .picks {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
  }
  .pick {
    display: grid;
    justify-items: center;
    gap: 3px;
    padding: 12px 8px 12px;
    text-align: center;
    border-radius: var(--radius-card);
    background: none;
    min-width: 0;
  }
  .pick {
    transition: transform 0.15s;
  }
  .pick:hover:not(:disabled) {
    transform: translateY(-2px);
  }
  .pick :global(.wrap),
  .pick :global(svg) {
    margin-bottom: 6px;
  }
  .pick b {
    max-width: 100%;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pick .sub {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .pick.off {
    opacity: 0.45;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--control);
  }
  .dot.live {
    background: var(--on);
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation: none;
    }
  }
</style>
