<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import Dialog from '$lib/Dialog.svelte';
  import Thumb from '$lib/Thumb.svelte';
  import { api, errText, type Household, type ShelfItem } from '$lib/api';
  import { size } from '$lib/devices';

  let {
    household,
    dragging,
    say,
    onlistenDrop
  }: {
    household: Household | null;
    dragging: boolean;
    say: (t: string, err?: boolean) => void;
    /** The page hands files dropped on the window to this handler (when it is over the shelf). */
    onlistenDrop: (handler: ((paths: string[]) => void) | null) => void;
  } = $props();

  let items = $state<ShelfItem[]>([]);
  let loaded = $state(false);
  let busy = $state(0);
  let preview = $state<ShelfItem | null>(null);
  let dragId = $state<string | null>(null);
  let all = $state(false);
  const PER_ROW = 6;
  const visible = $derived(all ? items : items.slice(0, PER_ROW));

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
      for (const p of paths) await put(() => api.shelfPutPath(p));
    });
    return () => {
      clearInterval(t);
      onlistenDrop(null);
    };
  });

  async function put(fn: () => Promise<unknown>) {
    busy++;
    try {
      if (await fn()) say('Put on your shelf');
    } catch (e) {
      say(errText(e), true);
    } finally {
      busy--;
      load();
    }
  }

  async function save(i: ShelfItem) {
    try {
      const path = await api.shelfGet(i.id);
      say(`Saved ${i.name} in your Zima Connect folder`);
      api.reveal(path);
    } catch (e) {
      say(errText(e), true);
    }
  }

  async function remove(i: ShelfItem) {
    preview = null;
    try {
      await api.shelfDelete(i.id);
      say('Removed from your shelf');
    } catch (e) {
      say(errText(e), true);
    }
    load();
  }

  // Dragging a card out of the window starts a native file drag (to the desktop, a folder, another app).
  // The file is fetched as soon as the mouse goes down, and the drag starts once it moved a little.
  let press: { x: number; y: number; item: ShelfItem; ready: Promise<unknown> } | null = null;

  function down(e: PointerEvent, item: ShelfItem) {
    if (e.button !== 0 || dragId) return;
    press = { x: e.clientX, y: e.clientY, item, ready: api.shelfCache(item.id, item.name, item.size).catch(() => null) };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', release);
    window.addEventListener('pointercancel', release);
  }
  function release() {
    press = null;
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', release);
    window.removeEventListener('pointercancel', release);
  }
  async function move(e: PointerEvent) {
    if (!press || dragId) return;
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 6) return;
    const { item, ready } = press;
    release();
    dragId = item.id;
    try {
      await ready;
      await api.dragOut(item.id, item.name, item.size);
    } catch (err) {
      say(errText(err), true);
    } finally {
      dragId = null;
    }
  }
</script>

<section class="droppable" class:over={dragging && items.length > 0} aria-label="Your shelf">
  <div class="panel-head">
    <span class="meta">Only you · reachable from all your devices</span>
    <button class="btn btn-primary" disabled={!household} onclick={() => put(() => api.shelfPut())}><Icon name="plus" size={15} /> Add files…</button>
  </div>

  {#if !household}
    <div class="empty-line" style="border-top:0;padding-left:0">The shelf lives on your Zima Connect server. Join the server first (Settings, Add device).</div>
  {:else if items.length === 0}
    <div class="dropzone" class:over={dragging}>
      <Icon name="upload" size={22} />
      <b>{busy ? 'Putting files on the shelf…' : 'Drop files here'}</b>
      <span class="sub">Keep a file here and pick it up on your phone or another computer. Drag it back out to save it anywhere.</span>
    </div>
  {:else}
    <div class="tile-grid" role="list">
      {#each visible as i (i.id)}
        <div class="tile" role="listitem" class:dragging={dragId === i.id} onpointerdown={(e) => down(e, i)}>
          <button class="open" onclick={() => (preview = i)} aria-label="Preview {i.name}"><Thumb item={i} /></button>
          <div class="cap">
            <div class="n" title={i.name}>{i.name}</div>
            <div class="sub num">{size(i.size)}</div>
          </div>
          <div class="acts">
            <button class="mini" onclick={() => save(i)} aria-label="Save {i.name}" title="Save to Downloads"><Icon name="download" size={14} /></button>
            <button class="mini" onclick={() => remove(i)} aria-label="Remove {i.name}" title="Remove"><Icon name="trash" size={14} /></button>
          </div>
        </div>
      {/each}
      {#if busy}
        <div class="tile loading"><div class="thumb-ph"></div><div class="cap"><div class="n">Uploading…</div></div></div>
      {/if}
    </div>
    <div class="panel-foot">
      {#if items.length > PER_ROW}<button class="link" onclick={() => (all = !all)}>{all ? 'Show less' : `Show all ${items.length}`}</button>{/if}
      <span class="sub">Drop files here to add them. Drag a file out to save it anywhere.</span>
    </div>
  {/if}
</section>

{#if preview}
  <Dialog title={preview.name} onclose={() => (preview = null)}>
    <Thumb item={preview} big />
    <div class="sub num" style="margin-top:8px">{size(preview.size)}</div>
    <div class="actions">
      <button class="btn btn-danger" onclick={() => remove(preview!)}>Remove</button>
      <button class="btn btn-secondary" onclick={() => (preview = null)}>Close</button>
      <button class="btn btn-primary" onclick={() => save(preview!)}><Icon name="download" size={15} /> Save</button>
    </div>
  </Dialog>
{/if}

<style>
  .tile {
    padding: 0;
    cursor: grab;
    touch-action: none;
  }
  .tile :global(.thumb) {
    transition: transform 0.15s;
  }
  .tile:hover :global(.thumb) {
    transform: translateY(-2px);
  }
  .tile.dragging {
    opacity: 0.5;
  }
  .open {
    display: block;
    width: 100%;
    padding: 0;
    cursor: pointer;
  }
  .loading {
    opacity: 0.6;
  }
  .thumb-ph {
    aspect-ratio: 4 / 3;
    border-radius: var(--radius-card);
    background: var(--raised);
    animation: shimmer 1.2s ease-in-out infinite;
  }
  @keyframes shimmer {
    50% {
      opacity: 0.5;
    }
  }
</style>
