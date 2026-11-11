<script lang="ts">
  import Icon from '$lib/Icon.svelte';
  import ShelfGrid from '$lib/ShelfGrid.svelte';
  import SendPanel from '$lib/SendPanel.svelte';
  import Dialog from '$lib/Dialog.svelte';
  import { on } from '$lib/prefs.svelte';
  import DeviceCard from '$lib/DeviceCard.svelte';
  import type { Activity, Contact, Household, LsPeer } from '$lib/api';

  let {
    contacts,
    household,
    activities,
    nearby,
    dragging,
    overId,
    now,
    say,
    onOpenDevice,
    onAdd,
    onShowActivity,
    onlistenDrop,
    onReveal,
    onRefresh
  }: {
    contacts: Contact[];
    household: Household | null;
    activities: Activity[];
    nearby: LsPeer[];
    dragging: boolean;
    overId: string | null;
    now: number;
    say: (t: string, err?: boolean) => void;
    onOpenDevice: (id: string) => void;
    onAdd: () => void;
    onShowActivity: () => void;
    onlistenDrop: (h: ((paths: string[]) => void) | null) => void;
    onReveal: (path: string) => void;
    onRefresh: () => void;
  } = $props();

  // Shelf and Send open as popups.
  let panel = $state<'shelf' | 'send' | null>(null);

  // Filter the devices: everything, mine, online, or one person's.
  let filter = $state('all');
  const people = $derived([...new Set(contacts.filter((c) => !c.mine && c.user_name).map((c) => c.user_name!))]);
  const shown = $derived(
    contacts.filter((c) =>
      filter === 'all' ? true : filter === 'mine' ? c.mine : filter === 'online' ? c.online : c.user_name === filter
    )
  );
  const online = $derived(contacts.filter((c) => c.online).length);
  const away = $derived(contacts.filter((c) => !c.online).map((c) => c.name));

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
</script>

<div class="title-row" style="margin-bottom:36px">
  <div>
    <h1>Home</h1>
    <div class="finding num">
      {#if !contacts.length}No other devices yet{:else if !away.length}{contacts.length === 1 ? 'Your device is' : `All ${contacts.length} devices are`} connected{:else}{online} of {contacts.length} devices online · {away.slice(0, 2).join(', ')}{away.length > 2 ? ` and ${away.length - 2} more` : ''} offline{/if}
    </div>
  </div>
  <div class="actions">
    {#if on('show_shelf')}<button class="btn btn-secondary" onclick={() => (panel = 'shelf')}><Icon name="shelf" size={15} /> Shelf</button>{/if}
    {#if on('show_send')}<button class="btn btn-primary" onclick={() => (panel = 'send')}><Icon name="upload" size={15} /> Send</button>{/if}
    <button class="btn btn-secondary" onclick={onAdd}><Icon name="plus" size={15} /> Add device</button>
  </div>
</div>

{#if panel}
  <Dialog wide title={panel === 'shelf' ? 'Your shelf' : 'Send'} onclose={() => (panel = null)}>
    {#if panel === 'shelf'}
      <ShelfGrid {household} {dragging} {say} {onlistenDrop} />
    {:else}
      <SendPanel {contacts} {nearby} {dragging} {say} refresh={onRefresh} {onlistenDrop} />
    {/if}
    <div class="actions"><button class="btn btn-secondary" onclick={() => (panel = null)}>Close</button></div>
  </Dialog>
{/if}

<section aria-labelledby="h-dev" class="block">
  <div class="head">
    <h2 id="h-dev">Devices</h2>
    {#if contacts.length > 1}
      <div class="chips" role="group" aria-label="Filter devices">
        <button class="chip" class:on={filter === 'all'} aria-pressed={filter === 'all'} onclick={() => (filter = 'all')}>All</button>
        <button class="chip" class:on={filter === 'mine'} aria-pressed={filter === 'mine'} onclick={() => (filter = 'mine')}>Mine</button>
        {#each people as p}
          <button class="chip" class:on={filter === p} aria-pressed={filter === p} onclick={() => (filter = p)}>{p}</button>
        {/each}
        <button class="chip" class:on={filter === 'online'} aria-pressed={filter === 'online'} onclick={() => (filter = 'online')}>Online</button>
      </div>
    {/if}
  </div>
  {#if contacts.length === 0}
    <div class="empty-line" style="border-top:0;padding-left:0">
      No other devices yet. {#if household}Ask the server owner for an invitation, or{:else}Join your server, or{/if}
      <button class="link" onclick={onAdd}>add a device</button>.
    </div>
  {:else}
    <div class="devgrid">
      {#each shown as c (c.id)}
        <DeviceCard {c} over={dragging && overId === c.id} onopen={() => onOpenDevice(c.id)} />
      {:else}
        <div class="sub">No devices match this filter.</div>
      {/each}
    </div>
    {#if dragging}<div class="sub hint">Drop files on a device to send them right away.</div>{/if}
  {/if}
</section>

{#if on('show_activity')}
<section aria-labelledby="h-act" class="block">
  <div class="head">
    <h2 id="h-act">Activity</h2>
    {#if activities.length > 6}<button class="link" onclick={onShowActivity}>Show all</button>{/if}
  </div>
  <div class="list">
    {#each activities.slice(0, 6) as a, n (a.id)}
      <div class="row" class:first={n === 0}>
        <Icon name={a.ok ? activityIcon(a.kind) : 'alert'} size={18} />
        <div class="grow"><div class="name">{a.title}</div><div class="sub">{a.detail}</div></div>
        {#if a.path}<button class="link" onclick={() => onReveal(a.path!)}>Show</button>{/if}
        <span class="sub num">{ago(a.ts)}</span>
      </div>
    {:else}
      <div class="empty-line" style="border-top:0;padding-left:0">Nothing yet. Things you send and receive show up here.</div>
    {/each}
  </div>
</section>
{/if}

<style>
  .block {
    margin-bottom: 44px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 16px;
    flex-wrap: wrap;
  }
  .head h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .list {
    border-top: 1px solid var(--rule);
  }
  .list :global(.row) {
    padding-left: 0;
    padding-right: 0;
  }
  .list :global(.row.first) {
    border-top: 0;
  }
  .head .link {
    margin-left: auto;
  }
  .chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-left: auto;
  }
  .chip {
    height: 26px;
    padding: 0 11px;
    border-radius: 99px;
    box-shadow: inset 0 0 0 1px var(--control);
    background: var(--surface);
    font-size: 13px;
  }
  .chip.on {
    background: var(--primary);
    color: var(--primary-ink);
    box-shadow: none;
  }
  .devgrid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 14px;
  }
  .hint {
    margin-top: 8px;
  }
  .row.first {
    border-top: 0;
  }
</style>
