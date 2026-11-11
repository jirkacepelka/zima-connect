<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import { prefs as shared } from '$lib/prefs.svelte';
  import { PALETTE, api, errText, type Household, type Settings } from '$lib/api';

  let {
    name,
    household,
    say,
    onRenamed,
    onAddDevice,
    onLeave,
    onCheckUpdate
  }: {
    name: string;
    household: Household | null;
    say: (t: string, err?: boolean) => void;
    onRenamed: (name: string) => void;
    onAddDevice: () => void;
    onLeave: () => void;
    onCheckUpdate: () => void;
  } = $props();

  let version = $state('');

  let prefs = $state<Settings | null>(null);
  let editing = $state(false);
  let draft = $state('');

  onMount(() => {
    api.settings().then((s) => (prefs = s));
    api.appVersion().then((v) => (version = v));
  });

  async function save(patch: Partial<Settings>) {
    if (!prefs) return;
    const next = { ...prefs, ...patch };
    try {
      prefs = await api.setSettings(next);
      shared.v = prefs;
    } catch (e) {
      say(errText(e), true);
    }
  }

  async function pickColour(h: number) {
    try {
      await api.setMyColor(h);
      say('Colour saved');
    } catch (e) {
      say(errText(e), true);
    }
  }

  async function rename() {
    const n = draft.trim();
    editing = false;
    if (!n || n === name) return;
    try {
      onRenamed(await api.renameDevice(n));
      say('Renamed. Everyone sees the new name.');
    } catch (e) {
      say(errText(e), true);
    }
  }
</script>

<div class="title-row">
  <div><h1>This computer</h1><div class="finding">How Zima Connect behaves on this computer.</div></div>
</div>

<section class="card" aria-labelledby="h-name">
  <div class="card-head"><h2 id="h-name">Name</h2></div>
  <div class="row">
    <Icon name="laptop" size={18} />
    {#if editing}
      <form
        class="grow edit"
        onsubmit={(e) => {
          e.preventDefault();
          rename();
        }}
      >
        <input class="field" bind:value={draft} maxlength="40" aria-label="Device name" />
        <button class="btn btn-primary">Save</button>
        <button type="button" class="btn btn-secondary" onclick={() => (editing = false)}>Cancel</button>
      </form>
    {:else}
      <div class="grow"><div class="name">{name}</div><div class="sub">Other devices and the server show this name.</div></div>
      <button
        class="btn btn-secondary"
        onclick={() => {
          draft = name;
          editing = true;
        }}><Icon name="pencil" size={15} /> Rename</button
      >
    {/if}
  </div>
</section>

{#if household}
  <section class="card" aria-labelledby="h-col">
    <div class="card-head"><h2 id="h-col">Your colour</h2></div>
    <div class="row">
      <div class="grow"><div class="name">Shown for you everywhere</div><div class="sub">On the server, on computers and phones.</div></div>
      <div class="swatches" role="radiogroup" aria-label="Your colour">
        {#each PALETTE as h}
          <button class="swatch" role="radio" aria-checked={household.info.me.hue === h} aria-label="Colour {h}" style="--h:{h}" onclick={() => pickColour(h)}></button>
        {/each}
      </div>
    </div>
  </section>
{/if}

<section class="card" aria-labelledby="h-bg">
  <div class="card-head"><h2 id="h-bg">Background</h2></div>
  {#if prefs}
    <div class="row">
      <div class="grow"><div class="name">Keep running when the window is closed</div><div class="sub">Files, clipboard and “find my device” keep working. Zima Connect stays in the tray.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Keep running in the tray" checked={prefs.background} onchange={(e) => save({ background: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Start with this computer</div><div class="sub">Starts hidden in the tray when you sign in.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Start with this computer" checked={prefs.autostart} onchange={(e) => save({ autostart: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Allow remote control</div><div class="sub">Other devices may use their touchpad, keyboard and media keys here, only where your household rules allow it.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Allow remote control" checked={prefs.allow_remote} onchange={(e) => save({ allow_remote: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Visible to LocalSend</div><div class="sub">People nearby with the LocalSend app can send you files. You always get asked first.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Visible to LocalSend" checked={prefs.localsend} onchange={(e) => save({ localsend: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Share files with my other devices</div><div class="sub">Lets your own devices look at and copy your Desktop, Documents, Downloads, Pictures, Music and Videos. Read-only, and only where your household rules allow it.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Share files" checked={prefs.share_files} onchange={(e) => save({ share_files: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Visible to Quick Share</div><div class="sub">Android phones nearby can send you files, texts and links. You see a PIN and always get asked first.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Visible to Quick Share" checked={prefs.quickshare} onchange={(e) => save({ quickshare: e.currentTarget.checked })} /><i></i></label>
    </div>
  {/if}
</section>

<section class="card" aria-labelledby="h-mod">
  <div class="card-head"><h2 id="h-mod">Modules</h2></div>
  <div class="sub" style="padding:0 20px 6px">Turn off what you do not use. Remote control, LocalSend and Quick Share are under Background.</div>
  {#if prefs}
    <div class="sub" style="padding:14px 20px 4px;font-weight:600">Show on this computer</div>
    <div class="row">
      <div class="grow"><div class="name">Shelf</div><div class="sub">Keep files for your own devices on the server.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Shelf" checked={prefs.show_shelf} onchange={(e) => save({ show_shelf: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Send</div><div class="sub">The place to drop files, text and links and pick who gets them.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Send" checked={prefs.show_send} onchange={(e) => save({ show_send: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Activity</div><div class="sub">The list of what was sent and received.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Activity" checked={prefs.show_activity} onchange={(e) => save({ show_activity: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="sub" style="padding:14px 20px 4px;font-weight:600">Receive from other devices</div>
    <div class="row">
      <div class="grow"><div class="name">Clipboard</div><div class="sub">Text other devices send lands in this computer’s clipboard.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Clipboard" checked={prefs.clipboard_in} onchange={(e) => save({ clipboard_in: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Links</div><div class="sub">Web links other devices send open in your browser.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Links" checked={prefs.links_in} onchange={(e) => save({ links_in: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Find my device</div><div class="sub">Let other devices make this computer ring.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Find my device" checked={prefs.ring_in} onchange={(e) => save({ ring_in: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Notifications</div><div class="sub">Show notifications mirrored from your phone.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Notifications" checked={prefs.notifications_in} onchange={(e) => save({ notifications_in: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="sub" style="padding:14px 20px 4px;font-weight:600">Phones</div>
    <div class="row">
      <div class="grow"><div class="name">Phone calls</div><div class="sub">Dial through a phone and see who is calling.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Phone calls" checked={prefs.phone_calls} onchange={(e) => save({ phone_calls: e.currentTarget.checked })} /><i></i></label>
    </div>
    <div class="row">
      <div class="grow"><div class="name">Text messages</div><div class="sub">Read and answer a phone’s SMS here.</div></div>
      <label class="switch"><input type="checkbox" role="switch" aria-label="Text messages" checked={prefs.sms} onchange={(e) => save({ sms: e.currentTarget.checked })} /><i></i></label>
    </div>
  {/if}
</section>

<section class="card" aria-labelledby="h-ver">
  <div class="card-head"><h2 id="h-ver">Updates</h2></div>
  <div class="row">
    <div class="grow"><div class="name">Zima Connect {version}</div><div class="sub">New versions are found automatically and installed with one click, no reinstalling.</div></div>
    <button class="btn btn-secondary" onclick={onCheckUpdate}>Check for updates</button>
  </div>
</section>

<section class="card" aria-labelledby="h-srv">
  <div class="card-head"><h2 id="h-srv">Server</h2></div>
  {#if household}
    <div class="row">
      <Icon name="server" size={18} />
      <div class="grow"><div class="name">{household.info.household}</div><div class="sub">You are {household.info.me.name}{household.info.can_admin ? ', you can manage the household' : ''}.</div></div>
      <button class="btn btn-danger" onclick={onLeave}>Leave</button>
    </div>
  {:else}
    <div class="row">
      <Icon name="server" size={18} />
      <div class="grow"><div class="name">Not connected to a server</div><div class="sub">Join once and your devices find each other automatically.</div></div>
    </div>
  {/if}
  <div class="row">
    <Icon name="plus" size={18} />
    <div class="grow"><div class="name">Add a device</div><div class="sub">Join a server, show a pairing code or enter one.</div></div>
    <button class="btn btn-secondary" onclick={onAddDevice}>Add device…</button>
  </div>
</section>

<style>
  .card {
    margin-bottom: 16px;
  }
  .edit {
    display: flex;
    gap: 8px;
  }
  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    max-width: 250px;
    justify-content: flex-end;
  }
  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: hsl(var(--h) 70% 62%);
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
  }
  .swatch[aria-checked='true'] {
    box-shadow: 0 0 0 2px var(--surface), 0 0 0 4px hsl(var(--h) 60% 45%);
  }
</style>
