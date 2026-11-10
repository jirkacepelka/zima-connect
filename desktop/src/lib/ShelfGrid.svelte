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
  let press: { x: number; y: number; item: ShelfItem } | null = null;
  function down(e: PointerEvent, item: ShelfItem) {
    if (e.button !== 0) return;
    press = { x: e.clientX, y: e.clientY, item };
  }
  async function move(e: PointerEvent) {
    if (!press || dragId) return;
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 8) return;
    const it = press.item;
    press = null;
    dragId = it.id;
    try {
      await api.dragOut(it.id, it.name, it.size);
    } catch (err) {
      say(errText(err), true);
    } finally {
      dragId = null;
    }
  }
  const up = () => (press = null);
</script>

<section class="shelf" class:over={dragging} aria-labelledby="h-shelf">
  <div class="head">
    <h2 id="h-shelf">Your shelf</h2>
    <span class="meta">Only you · reachable from all your devices</span>
    <button class="btn btn-secondary add" disabled={!household} onclick={() => put(() => api.shelfPut())}><Icon name="plus" size={15} /> Add files…</button>
  </div>

  {#if !household}
    <div class="empty-line" style="border-top:0;padding-left:0">The shelf lives on your Zima Connect server. Join the server first (Settings, Add device).</div>
  {:else if items.length === 0}
    <div class="empty" class:over={dragging}>
      <Icon name="upload" size={22} />
      <b>{busy ? 'Putting files on the shelf…' : 'Drop files here'}</b>
      <span class="sub">Keep a file here and pick it up on your phone or another computer. Drag it back out to save it anywhere.</span>
    </div>
  {:else}
    <div class="grid" role="list">
      {#each items as i (i.id)}
        <div class="tile" role="listitem" class:dragging={dragId === i.id} onpointerdown={(e) => down(e, i)} onpointermove={move} onpointerup={up} onpointercancel={up}>
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
    <div class="foot sub">Drop more files anywhere on this card. Drag a file out to save it.</div>
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
  .shelf {
    margin-bottom: 44px;
    border-radius: 14px;
    transition: box-shadow 0.12s, background 0.12s;
  }
  .shelf.over {
    box-shadow: 0 0 0 2px var(--hue);
    background: var(--hue-pale);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
  }
  .head h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .head .meta {
    color: var(--muted);
    font-size: 13px;
    margin-left: auto;
  }
  .empty {
    padding: 40px 20px;
    display: grid;
    justify-items: center;
    gap: 6px;
    text-align: center;
    border: 1.5px dashed var(--control);
    border-radius: 14px;
    color: var(--muted);
  }
  .empty b {
    color: var(--text);
  }
  .empty.over {
    border-color: var(--hue);
    background: var(--hue-pale);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(138px, 1fr));
    gap: 20px 16px;
    padding: 0;
  }
  .tile {
    position: relative;
    display: grid;
    gap: 8px;
    padding: 0;
    cursor: grab;
    touch-action: none;
  }
  .tile :global(.thumb) {
    border-radius: 10px;
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
  .cap {
    padding: 0 2px;
    min-width: 0;
  }
  .n {
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .acts {
    position: absolute;
    top: 10px;
    right: 10px;
    display: flex;
    gap: 4px;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .tile:hover .acts,
  .tile:focus-within .acts {
    opacity: 1;
  }
  .mini {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 6px;
    background: var(--surface);
    box-shadow: 0 0 0 1px var(--border);
  }
  .mini:hover {
    background: var(--raised);
  }
  .loading {
    opacity: 0.6;
  }
  .thumb-ph {
    aspect-ratio: 4 / 3;
    border-radius: 6px;
    background: var(--raised);
    animation: shimmer 1.2s ease-in-out infinite;
  }
  @keyframes shimmer {
    50% {
      opacity: 0.5;
    }
  }
  .foot {
    padding: 14px 0 0;
  }
</style>
