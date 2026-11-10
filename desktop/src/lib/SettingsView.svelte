<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import { api, errText, type Household, type Settings } from '$lib/api';

  let {
    name,
    household,
    say,
    onRenamed,
    onAddDevice,
    onLeave
  }: {
    name: string;
    household: Household | null;
    say: (t: string, err?: boolean) => void;
    onRenamed: (name: string) => void;
    onAddDevice: () => void;
    onLeave: () => void;
  } = $props();

  let prefs = $state<Settings | null>(null);
  let editing = $state(false);
  let draft = $state('');

  onMount(() => {
    api.settings().then((s) => (prefs = s));
  });

  async function save(patch: Partial<Settings>) {
    if (!prefs) return;
    const next = { ...prefs, ...patch };
    try {
      prefs = await api.setSettings(next);
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
  {/if}
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
</style>
