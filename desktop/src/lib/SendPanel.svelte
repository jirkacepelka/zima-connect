<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import Dialog from '$lib/Dialog.svelte';
  import { api, errText, type Contact, type LsPeer } from '$lib/api';
  import { hueFor, size } from '$lib/devices';

  let {
    contacts,
    nearby,
    dragging,
    say,
    refresh,
    onlistenDrop
  }: {
    contacts: Contact[];
    nearby: LsPeer[];
    dragging: boolean;
    say: (t: string, err?: boolean) => void;
    refresh: () => void;
    onlistenDrop: (h: ((paths: string[]) => void) | null) => void;
  } = $props();

  type Staged = { id: number; kind: 'file' | 'text' | 'link'; label: string; detail: string; path?: string; value?: string; state: 'ready' | 'sending' | 'done' | 'failed' };
  let staged = $state<Staged[]>([]);
  let nextId = 1;
  let draft = $state('');
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

  onMount(() => {
    onlistenDrop(addPaths);
    return () => onlistenDrop(null);
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
      staged.push({ id: nextId++, kind: 'file', label: i.name, detail: size(i.size), path: i.path, state: 'ready' });
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
    draft = '';
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

<div class="send" class:over={dragging}>
  {#if staged.length === 0}
    <div class="zone" class:over={dragging}>
      <Icon name="upload" size={28} />
      <b>Drop files here to send them</b>
      <span class="sub">or choose them, or paste text or a link. You pick where it goes next.</span>
      <div class="btns">
        <button class="btn btn-primary" onclick={choose}>Choose files…</button>
        <button class="btn btn-secondary" onclick={async () => { try { addText(await navigator.clipboard.readText()); } catch { say('Could not read the clipboard', true); } }}>Paste from clipboard</button>
      </div>
    </div>
  {:else}
    <ul class="staged" aria-label="Ready to send">
      {#each staged as s (s.id)}
        <li class="item {s.state}">
          <span class="ico"><Icon name={s.kind === 'file' ? 'file' : s.kind === 'link' ? 'globe' : 'clipboard'} size={16} /></span>
          <span class="lbl"><b>{s.label}</b><span class="sub num">{s.detail}</span></span>
          {#if s.state === 'sending'}<span class="spin" aria-label="Sending"></span>
          {:else if s.state === 'done'}<span class="okm"><Icon name="check" size={16} /></span>
          {:else}<button class="x" onclick={() => remove(s.id)} aria-label="Remove {s.label}"><Icon name="x" size={14} /></button>{/if}
        </li>
      {/each}
    </ul>
    <form class="add" onsubmit={(e) => { e.preventDefault(); addText(draft); }}>
      <input class="field" bind:value={draft} placeholder="Add text or a link" aria-label="Text or link to add" autocomplete="off" />
      <button type="button" class="btn btn-secondary" onclick={choose}>Add files…</button>
      <button type="button" class="btn btn-primary" disabled={sending || !readyCount} onclick={() => (picking = true)}>{sending ? 'Sending…' : `Send ${readyCount}…`}</button>
    </form>
  {/if}
</div>

{#if picking}
  <Dialog title="Send to" onclose={() => (picking = false)}>
    <p class="lead"><b>Who gets {readyCount === 1 ? 'it' : `these ${readyCount} items`}?</b></p>
    <div class="to" role="group" aria-label="Send to">
      {#if contacts.length === 0 && nearby.length === 0}
        <div class="sub">No devices yet. Add one, or open LocalSend on a device nearby.</div>
      {/if}
      {#each [{ label: 'My devices', items: mine }, { label: 'Others in the household', items: others }] as g}
        {#if g.items.length}
          <div class="grp">
            <span class="g">{g.label}</span>
            {#each g.items as c (c.id)}
              <button class="pick" class:off={!reachable(c)} disabled={!reachable(c)} style="--h:{hueFor(c)}" onclick={() => choosePeer(c.id)}>
                <i class="dot" class:live={c.online}></i>{c.name}
              </button>
            {/each}
          </div>
        {/if}
      {/each}
      {#if nearby.length}
        <div class="grp">
          <span class="g">Nearby, with LocalSend</span>
          {#each nearby as p (p.fingerprint)}
            <button class="pick" onclick={() => choosePeer('ls:' + p.fingerprint)}>
              <Icon name={p.device_type === 'mobile' ? 'phone' : 'laptop'} size={14} />{p.alias}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <div class="actions"><button class="btn btn-secondary" onclick={() => (picking = false)}>Cancel</button></div>
  </Dialog>
{/if}

<style>
  .send {
    border-radius: 14px;
    transition: box-shadow 0.12s;
  }
  .send.over {
    box-shadow: 0 0 0 2px var(--primary);
  }
  .zone {
    display: grid;
    justify-items: center;
    gap: 6px;
    padding: 52px 20px;
    text-align: center;
    border: 1.5px dashed var(--control);
    border-radius: 14px;
    color: var(--muted);
  }
  .zone b {
    color: var(--text);
  }
  .zone.over {
    border-color: var(--primary);
  }
  .btns {
    display: flex;
    gap: 8px;
    margin-top: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }
  .staged {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-radius: 10px;
    background: var(--raised);
  }
  .item.failed {
    box-shadow: inset 0 0 0 1px #d63b3b;
  }
  .lbl {
    display: grid;
    min-width: 0;
    flex: 1;
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
  }
  .spin {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid var(--control);
    border-top-color: var(--primary);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .add {
    display: flex;
    gap: 8px;
  }
  .add .field {
    flex: 1;
  }
  .to {
    display: grid;
    gap: 10px;
  }
  .lbl-row {
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }
  .grp {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .g {
    font-size: 12px;
    color: var(--muted);
    min-width: 130px;
  }
  .pick {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 14px;
    border-radius: 99px;
    box-shadow: inset 0 0 0 1px var(--control);
    background: var(--surface);
    font-size: 14px;
  }
  .pick.on {
    background: var(--primary);
    color: var(--primary-ink);
    box-shadow: none;
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
    background: hsl(var(--h) 60% 48%);
  }
  .go {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation: none;
    }
  }
</style>
