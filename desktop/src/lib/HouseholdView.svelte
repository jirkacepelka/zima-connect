<script lang="ts">
  import Icon from './Icon.svelte';
  import { api, CAPS, allowed, defaultAllowed, errText, type Cap, type Household, type User } from './api';

  let {
    household,
    myDeviceId,
    say,
    onchange,
    oninvite
  }: {
    household: Household;
    myDeviceId: string;
    say: (t: string, error?: boolean) => void;
    onchange: () => void;
    oninvite: (code: string, label: string) => void;
  } = $props();

  const snap = $derived(household.snapshot);
  const me = $derived(household.info.me);
  const admin = $derived(household.info.can_admin);
  const others = $derived(snap.users.filter((u) => u.id !== me.id));
  const pairs = $derived(
    admin
      ? snap.users.flatMap((a) => snap.users.filter((b) => b.id !== a.id).map((b) => [a, b] as const))
      : others.flatMap((o) => [[me, o] as const, [o, me] as const])
  );

  let newPerson = $state('');
  let busy = $state(false);

  const who = (u: User) => (u.id === me.id ? 'You' : u.name);
  const whose = (u: User) => (u.id === me.id ? 'your' : `${u.name}’s`);

  async function run(fn: () => Promise<unknown>, ok?: string) {
    if (busy) return;
    busy = true;
    try {
      await fn();
      if (ok) say(ok);
      onchange();
    } catch (e) {
      say(errText(e), true);
    } finally {
      busy = false;
    }
  }

  async function invite(userId: string | null, name: string | null, label: string) {
    if (busy) return;
    busy = true;
    try {
      const code = await api.admin({ op: 'create_invite', user_id: userId, new_user_name: name });
      if (code) oninvite(code, label);
      newPerson = '';
      onchange();
    } catch (e) {
      say(errText(e), true);
    } finally {
      busy = false;
    }
  }

  function toggle(from: User, to: User, cap: Cap, value: boolean) {
    run(() =>
      value === defaultAllowed(from.id, to.id, cap)
        ? api.admin({ op: 'clear_rule', from_user: from.id, to_user: to.id, cap })
        : api.admin({ op: 'set_rule', from_user: from.id, to_user: to.id, cap, allow: value })
    );
  }

  function removeDevice(id: string, name: string) {
    if (!confirm(`Remove “${name}” from the household?`)) return;
    run(() => api.admin({ op: 'remove_device', id }), `${name} removed`);
  }

  function removeUser(u: User) {
    if (!confirm(`Remove ${u.name} and all their devices?`)) return;
    run(() => api.admin({ op: 'remove_user', id: u.id }), `${u.name} removed`);
  }

  function leave() {
    if (!confirm('Leave this household? You can join again with a new code.')) return;
    run(() => api.leave(), 'Left the household');
  }
</script>

<div class="wrap">
  <header class="head">
    <div class="head-icon"><Icon name="users" size={28} /></div>
    <div class="head-text">
      <h1>{household.info.household}</h1>
      <div class="sub">
        You are <b>{me.name}</b>{admin ? ' · Owner' : ''}
      </div>
    </div>
    <button class="ghost" onclick={leave}>Leave</button>
  </header>

  <h2>People</h2>
  <ul class="card list">
    {#each snap.users as u (u.id)}
      {@const devs = snap.devices.filter((d) => d.user_id === u.id)}
      <li class="person">
        <div class="p-top">
          <span class="avatar" style="--h:{u.hue ?? 222}">{u.name.slice(0, 1).toUpperCase()}</span>
          <span class="p-name">{u.id === me.id ? `${u.name} (you)` : u.name}</span>
          <span class="pill">{u.role === 'owner' ? 'Owner' : 'Member'}</span>
          <span class="spacer"></span>
          {#if admin}
            <button class="link" onclick={() => invite(u.id, null, `New device for ${u.name}`)}>
              <Icon name="plus" size={14} /> Add device
            </button>
            {#if u.role !== 'owner'}
              <button class="link danger" onclick={() => removeUser(u)} aria-label="Remove {u.name}">
                <Icon name="trash" size={14} />
              </button>
            {/if}
          {/if}
        </div>
        <ul class="devs">
          {#each devs as d (d.id)}
            <li>
              <Icon name={d.platform === 'android' ? 'phone' : d.platform === 'server' ? 'server' : 'laptop'} size={15} />
              <span>{d.name}{d.id === myDeviceId ? ' · this computer' : ''}</span>
              {#if admin && d.id !== myDeviceId}
                <button class="icon-btn" onclick={() => removeDevice(d.id, d.name)} aria-label="Remove {d.name}">
                  <Icon name="x" size={13} />
                </button>
              {/if}
            </li>
          {/each}
          {#if devs.length === 0}<li class="muted">No devices</li>{/if}
        </ul>
      </li>
    {/each}
  </ul>

  {#if admin}
    <form
      class="add-person"
      onsubmit={(e) => {
        e.preventDefault();
        if (newPerson.trim()) invite(null, newPerson.trim(), `Invitation for ${newPerson.trim()}`);
      }}
    >
      <input placeholder="Add a person, e.g. Mom" bind:value={newPerson} />
      <button class="primary" disabled={!newPerson.trim() || busy}>Create invitation</button>
    </form>
  {/if}

  {#if pairs.length}
    <h2>What people can do for each other</h2>
    <p class="note">
      {admin
        ? 'Rules apply between people. Their own devices always work together.'
        : 'These rules are set by the owner of the household.'}
    </p>
    <div class="policy">
      {#each pairs as [from, to] (from.id + to.id)}
        <section class="card pol">
          <div class="pol-title">
            <b>{who(from)}</b> <span class="arrow">→</span> <b>{who(to)}</b>
          </div>
          <div class="pol-sub">What {from.id === me.id ? 'you' : from.name} can do on {whose(to)} devices</div>
          {#each CAPS as c (c.id)}
            {@const on = allowed(snap, from.id, to.id, c.id)}
            <label class="switch-row" class:disabled={!admin || c.id === 'calls'}>
              <span>{c.label}</span>
              <input
                type="checkbox"
                role="switch"
                checked={on}
                disabled={!admin || busy || c.id === 'calls'}
                onchange={(e) => toggle(from, to, c.id, e.currentTarget.checked)}
              />
              <span class="track"><span class="knob"></span></span>
            </label>
          {/each}
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrap {
    max-width: 760px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 8px;
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
  }
  h1 {
    margin: 0;
    font-size: 24px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  h2 {
    font-size: 15px;
    font-weight: 650;
    margin: 30px 0 10px;
  }
  .sub,
  .note,
  .muted {
    color: var(--text-2);
  }
  .note {
    margin: -4px 0 12px;
    font-size: 13px;
  }
  .card {
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow), 0 0 0 1px var(--line);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 4px 0;
  }
  .person {
    padding: 12px 16px;
  }
  .person + .person {
    border-top: 1px solid var(--line);
  }
  .p-top {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .spacer {
    flex: 1;
  }
  .avatar {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: hsl(var(--h) 60% 90%);
    color: hsl(var(--h) 55% 28%);
    display: grid;
    place-items: center;
    font-weight: 650;
  }
  .p-name {
    font-weight: 600;
  }
  .pill {
    font-size: 11.5px;
    padding: 2px 9px;
    border-radius: 99px;
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .devs {
    list-style: none;
    margin: 8px 0 0 40px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .devs li {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
    font-size: 13px;
  }
  .add-person {
    display: flex;
    gap: 10px;
    margin-top: 12px;
  }
  .add-person input {
    flex: 1;
    font: inherit;
    padding: 9px 12px;
    border-radius: 10px;
    border: 0;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--line);
    user-select: text;
  }
  .add-person input:focus {
    outline: 2px solid var(--accent);
  }
  .policy {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 14px;
  }
  .pol {
    padding: 16px 18px 8px;
  }
  .pol-title {
    font-size: 15px;
  }
  .arrow {
    color: var(--text-3);
    margin: 0 4px;
  }
  .pol-sub {
    color: var(--text-2);
    font-size: 12.5px;
    margin: 2px 0 8px;
  }
  .switch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 9px 0;
    border-top: 1px solid var(--line);
    cursor: pointer;
    position: relative;
  }
  .switch-row.disabled {
    cursor: default;
    opacity: 0.55;
  }
  .switch-row input {
    position: absolute;
    opacity: 0;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    cursor: inherit;
  }
  .track {
    width: 40px;
    height: 24px;
    border-radius: 99px;
    background: var(--text-3);
    position: relative;
    transition: background 0.15s;
    flex-shrink: 0;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
    transition: transform 0.15s;
  }
  input:checked ~ .track {
    background: var(--ok);
  }
  input:checked ~ .track .knob {
    transform: translateX(16px);
  }
  input:focus-visible ~ .track {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .primary {
    background: var(--accent);
    color: #fff;
    padding: 8px 16px;
    border-radius: 9px;
    font-weight: 600;
  }
  .primary:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .ghost {
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
  .link.danger {
    color: var(--text-3);
  }
  .link.danger:hover {
    color: var(--warn);
    background: var(--hover);
  }
  .icon-btn {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .icon-btn:hover {
    background: var(--hover);
    color: var(--warn);
  }
</style>
