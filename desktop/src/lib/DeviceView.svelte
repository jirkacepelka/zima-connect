<script lang="ts">
  import Icon from '$lib/Icon.svelte';
  import RemotePad from '$lib/RemotePad.svelte';
  import { api, errText, type Cap, type Contact, type Delivery } from '$lib/api';

  let {
    c,
    myName,
    dragging,
    ringing,
    say,
    refresh,
    onUnpair,
    onRing,
    onStopRing
  }: {
    c: Contact;
    myName: string;
    dragging: boolean;
    ringing: boolean;
    say: (t: string, err?: boolean) => void;
    refresh: () => void;
    onUnpair: () => void;
    onRing: () => void;
    onStopRing: () => void;
  } = $props();

  let link = $state('');
  let busy = $state<string | null>(null);
  let remoteOpen = $state(false);

  // Household devices can be reached through the server even when offline (except live-only things).
  const live: Cap[] = ['ring', 'remote', 'status', 'calls'];
  const can = (cap: Cap) => c.caps.includes(cap) && (c.online || (!c.direct && !live.includes(cap)));
  const why = (cap: Cap) =>
    !c.caps.includes(cap)
      ? `Not allowed for ${c.user_name ?? 'this device'} by your household rules`
      : !can(cap)
        ? 'Device is not reachable right now'
        : '';

  async function run(key: string, fn: () => Promise<unknown>) {
    if (busy) return;
    busy = key;
    try {
      await fn();
    } catch (e) {
      say(errText(e), true);
    } finally {
      busy = null;
      refresh();
    }
  }
  const told = (d: Delivery | null, what: string) => {
    if (d) say(d === 'queued' ? `${what} queued: the server delivers it when the device is back` : `${what} sent`);
  };

  const sendFile = () => run('file', async () => told(await api.sendFile(c.id), 'File'));
  const sendClipboard = () => run('clip', async () => told(await api.sendClipboard(c.id), 'Clipboard'));
  const sendLink = () =>
    run('link', async () => {
      const u = link.trim();
      if (!u) return;
      await api.sendUrl(c.id, /^https?:\/\//i.test(u) ? u : `https://${u}`);
      link = '';
      say(`Link opened on ${c.name}`);
    });

  const platform: Record<string, string> = { android: 'Android', windows: 'Windows', linux: 'Linux', macos: 'macOS', ios: 'iOS', server: 'ZimaOS' };
  const icon = (p: string) => (p === 'android' || p === 'ios' ? 'phone' : p === 'server' ? 'server' : 'laptop');
</script>

<div class="title-row">
  <div class="head-icon"><Icon name={icon(c.platform)} size={22} /></div>
  <div>
    <h1>{c.name}</h1>
    <div class="finding num">
      <span class="dot" class:on={c.online}></span>
      {c.online ? `Connected${c.rtt_ms !== null ? `, ${c.rtt_ms} ms` : ''}` : 'Not reachable right now'}
      · {platform[c.platform] ?? c.platform}{c.user_name && !c.mine ? ` · ${c.user_name}’s device` : ''}
      {#if c.battery}
        · <Icon name="battery" size={14} /> {c.battery[0]} %{c.battery[1] ? ' charging' : ''}
      {/if}
    </div>
  </div>
  {#if c.direct}
    <div class="actions"><button class="btn btn-secondary" onclick={onUnpair}><Icon name="trash" size={15} /> Remove</button></div>
  {/if}
</div>

<div class="drop" class:over={dragging} class:off={!can('files')}>
  <Icon name="upload" size={20} />
  <div>
    <b>{can('files') ? `Drop files here to send them to ${c.name}` : 'Sending files is not available'}</b>
    <div class="sub">{can('files') ? 'Or use the buttons below. Large files are fine.' : why('files')}</div>
  </div>
</div>

<section class="card" aria-labelledby="h-send">
  <div class="card-head"><h2 id="h-send">Send</h2></div>
  <div class="row">
    <Icon name="file" size={18} />
    <div class="grow"><div class="name">File</div><div class="sub">{can('files') ? 'Photos, documents, anything' : why('files')}</div></div>
    <button class="btn btn-secondary" disabled={!can('files') || !!busy} onclick={sendFile}>Choose file…</button>
  </div>
  <div class="row">
    <Icon name="clipboard" size={18} />
    <div class="grow"><div class="name">Clipboard</div><div class="sub">{can('clipboard') ? 'Send what you copied on this computer' : why('clipboard')}</div></div>
    <button class="btn btn-secondary" disabled={!can('clipboard') || !!busy} onclick={sendClipboard}>Send clipboard</button>
  </div>
  <div class="row">
    <Icon name="globe" size={18} />
    <form
      class="grow link-form"
      onsubmit={(e) => {
        e.preventDefault();
        sendLink();
      }}
    >
      <label class="name" for="lnk">Open a link there</label>
      <input id="lnk" class="field" placeholder="example.org" bind:value={link} disabled={!can('clipboard')} autocomplete="off" />
    </form>
    <button class="btn btn-secondary" disabled={!can('clipboard') || !link.trim() || !!busy} onclick={sendLink}>Open</button>
  </div>
</section>

<section class="card" aria-labelledby="h-tools">
  <div class="card-head"><h2 id="h-tools">Tools</h2></div>
  <div class="row">
    <Icon name="bell" size={18} />
    <div class="grow"><div class="name">Find device</div><div class="sub">{can('ring') ? (ringing ? 'Ringing now. It stops by itself after 30 s.' : 'Make it ring, even when it is silent') : why('ring')}</div></div>
    {#if ringing}
      <button class="btn btn-primary" onclick={onStopRing}>Stop ringing</button>
    {:else}
      <button class="btn btn-secondary" disabled={!can('ring') || !!busy} onclick={onRing}>Ring</button>
    {/if}
  </div>
  <div class="row">
    <Icon name="touchpad" size={18} />
    <div class="grow"><div class="name">Remote control</div><div class="sub">{can('remote') ? 'Use this computer as a touchpad, keyboard and media remote' : why('remote')}</div></div>
    <button class="btn btn-secondary" disabled={!can('remote')} aria-expanded={remoteOpen} onclick={() => (remoteOpen = !remoteOpen)}>{remoteOpen ? 'Hide' : 'Open'}</button>
  </div>
  {#if remoteOpen && can('remote')}
    <RemotePad peerId={c.id} name={c.name} {say} />
  {/if}
</section>

<style>
  .head-icon {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    border: 1.5px solid var(--text);
    display: grid;
    place-items: center;
    flex: none;
  }
  .finding {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .drop {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 20px;
    margin-bottom: 16px;
    border: 1.5px dashed var(--control);
    border-radius: var(--radius-card);
    background: var(--surface);
    transition: border-color 0.12s, background 0.12s;
  }
  .drop.over {
    border-color: var(--hue);
    background: var(--hue-pale);
  }
  .drop.off {
    opacity: 0.6;
  }
  .card {
    margin-bottom: 16px;
  }
  .link-form {
    display: grid;
    gap: 6px;
  }
</style>
