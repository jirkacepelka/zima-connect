<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import { api, errText, type Audience, type Household, type ShelfItem } from '$lib/api';

  let {
    household,
    dragging,
    say,
    onlistenDrop
  }: {
    household: Household | null;
    dragging: boolean;
    say: (t: string, err?: boolean) => void;
    /** The page registers a handler here for files dropped onto the window. */
    onlistenDrop: (handler: ((paths: string[]) => void) | null) => void;
  } = $props();

  let items = $state<ShelfItem[]>([]);
  let loaded = $state(false);
  let who = $state<'everyone' | 'some'>('everyone');
  let picked = $state<string[]>([]);
  let busy = $state(false);

  const others = $derived(household ? household.snapshot.users.filter((u) => u.id !== household!.info.me.id) : []);
  const audience = $derived<Audience>(who === 'everyone' ? { to: 'everyone' } : { to: 'users', ids: picked });

  async function load() {
    try {
      items = await api.shelfList();
    } catch (e) {
      if (!loaded) say(errText(e), true);
    } finally {
      loaded = true;
    }
  }

  onMount(() => {
    load();
    const t = setInterval(load, 8000);
    onlistenDrop(async (paths) => {
      for (const p of paths) await put(() => api.shelfPutPath(audience, p));
    });
    return () => {
      clearInterval(t);
      onlistenDrop(null);
    };
  });

  async function put(fn: () => Promise<unknown>) {
    if (busy) return;
    busy = true;
    try {
      const r = await fn();
      if (r) say('Put on the shelf');
    } catch (e) {
      say(errText(e), true);
    } finally {
      busy = false;
      load();
    }
  }

  async function take(i: ShelfItem) {
    try {
      const path = await api.shelfGet(i.id);
      say(`Saved ${i.name}`);
      api.reveal(path);
    } catch (e) {
      say(errText(e), true);
    }
  }

  async function remove(i: ShelfItem) {
    try {
      await api.shelfDelete(i.id);
      say('Removed from the shelf');
    } catch (e) {
      say(errText(e), true);
    }
    load();
  }

  const size = (n: number) => (n > 1e9 ? `${(n / 1e9).toFixed(1)} GB` : n > 1e6 ? `${(n / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1e3))} kB`);
  function forWhom(a: Audience) {
    if (a.to === 'everyone') return 'everyone';
    const names = a.ids.map((id) => household?.snapshot.users.find((u) => u.id === id)?.name ?? 'someone');
    return names.join(', ') || 'only you';
  }
  const toggle = (id: string) => (picked = picked.includes(id) ? picked.filter((x) => x !== id) : [...picked, id]);
</script>

<div class="title-row">
  <div>
    <h1>Shelf</h1>
    <div class="finding">Leave files on the server for others to pick up, even when you are not online together.</div>
  </div>
</div>

{#if !household}
  <section class="card"><div class="empty-line" style="border-top:0">The shelf lives on your Zima Connect server. Join the server first (Settings, Add device).</div></section>
{:else}
  <div class="drop" class:over={dragging}>
    <Icon name="upload" size={20} />
    <div class="grow">
      <b>Drop files here to put them on the shelf</b>
      <div class="sub">Visible to {who === 'everyone' ? 'everyone you may share files with' : picked.length ? forWhom(audience) : 'nobody yet. Choose people below'}. Kept for 14 days.</div>
    </div>
    <button class="btn btn-primary" disabled={busy || (who === 'some' && !picked.length)} onclick={() => put(() => api.shelfPut(audience))}>Choose file…</button>
  </div>

  <fieldset class="aud">
    <legend class="sr">Who can take it</legend>
    <label class="opt"><input type="radio" bind:group={who} value="everyone" /> Everyone</label>
    <label class="opt"><input type="radio" bind:group={who} value="some" /> Only…</label>
    {#if who === 'some'}
      {#each others as u (u.id)}
        <button class="chip" class:on={picked.includes(u.id)} aria-pressed={picked.includes(u.id)} onclick={() => toggle(u.id)}>{u.name}</button>
      {:else}
        <span class="sub">No other people in the household yet.</span>
      {/each}
    {/if}
  </fieldset>

  <section class="card" aria-labelledby="h-shelf">
    <div class="card-head"><h2 id="h-shelf">On the shelf</h2><span class="meta num">{items.length ? `${items.length} ${items.length === 1 ? 'file' : 'files'}` : ''}</span></div>
    {#each items as i (i.id)}
      <div class="row">
        <Icon name="file" size={18} />
        <div class="grow">
          <div class="name">{i.name}</div>
          <div class="sub num">{i.mine ? 'You' : i.owner_user_name} for {forWhom(i.audience)} · {size(i.size)}</div>
        </div>
        <button class="btn btn-secondary" onclick={() => take(i)}><Icon name="download" size={15} /> Take</button>
        {#if i.mine}<button class="link-danger" onclick={() => remove(i)}>Remove</button>{/if}
      </div>
    {:else}
      <div class="empty-line">{loaded ? 'Nothing on the shelf. Files you or others put here show up in this list.' : 'Loading…'}</div>
    {/each}
  </section>
  <p class="note">Files on the shelf are stored on your server so it can show them to the right people. Direct sends and the offline queue stay end-to-end encrypted.</p>
{/if}

<style>
  .drop {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 20px;
    margin-bottom: 12px;
    border: 1.5px dashed var(--control);
    border-radius: var(--radius-card);
    background: var(--surface);
    transition: border-color 0.12s, background 0.12s;
  }
  .drop.over {
    border-color: var(--hue);
    background: var(--hue-pale);
  }
  .aud {
    border: 0;
    padding: 0;
    margin: 0 0 16px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
  }
  .opt {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    cursor: pointer;
  }
  .opt input {
    accent-color: var(--primary);
  }
  .chip {
    height: 28px;
    padding: 0 12px;
    border-radius: 99px;
    box-shadow: inset 0 0 0 1px var(--control);
    background: var(--surface);
  }
  .chip.on {
    background: var(--primary);
    color: var(--primary-ink);
    box-shadow: none;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .note {
    color: var(--muted);
    font-size: 13px;
    margin: 12px 2px 0;
  }
</style>
