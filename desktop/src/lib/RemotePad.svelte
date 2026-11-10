<script lang="ts">
  import Icon from '$lib/Icon.svelte';
  import { api, errText, type RemoteInput } from '$lib/api';

  let { peerId, name, say }: { peerId: string; name: string; say: (t: string, err?: boolean) => void } = $props();

  let text = $state('');
  let active = false;
  let last = { x: 0, y: 0 };
  let acc = { x: 0, y: 0, scroll: 0 };
  let moved = 0;
  let inflight = false;
  let timer: ReturnType<typeof setInterval> | null = null;
  let downAt = 0;

  async function send(input: RemoteInput, quiet = false) {
    try {
      await api.remote(peerId, input);
    } catch (e) {
      if (!quiet) say(errText(e), true);
    }
  }

  // Pointer movement is batched: one message every 40 ms, never more than one in flight.
  function flush() {
    if (inflight) return;
    const dx = Math.round(acc.x);
    const dy = Math.round(acc.y);
    const sc = Math.round(acc.scroll);
    if (!dx && !dy && !sc) return;
    acc = { x: acc.x - dx, y: acc.y - dy, scroll: acc.scroll - sc };
    inflight = true;
    const job = sc ? send({ k: 'scroll', dy: sc }, true) : send({ k: 'move', dx, dy }, true);
    job.finally(() => (inflight = false));
  }

  function down(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    active = true;
    moved = 0;
    downAt = Date.now();
    last = { x: e.clientX, y: e.clientY };
    timer = setInterval(flush, 40);
  }
  function move(e: PointerEvent) {
    if (!active) return;
    const dx = (e.clientX - last.x) * 1.6;
    const dy = (e.clientY - last.y) * 1.6;
    last = { x: e.clientX, y: e.clientY };
    moved += Math.abs(dx) + Math.abs(dy);
    // Two fingers or the right button scroll; a plain drag moves the pointer.
    if (e.buttons & 2) acc.scroll += dy / 4;
    else {
      acc.x += dx;
      acc.y += dy;
    }
  }
  function up() {
    if (!active) return;
    active = false;
    if (timer) clearInterval(timer);
    flush();
    if (moved < 6 && Date.now() - downAt < 350) send({ k: 'click', button: 'left' });
  }
  function wheel(e: WheelEvent) {
    e.preventDefault();
    acc.scroll += e.deltaY / 40;
    flush();
  }

  const media = (action: 'play_pause' | 'next' | 'prev' | 'vol_up' | 'vol_down' | 'mute') => send({ k: 'media', action });
  const key = (k: string) => send({ k: 'key', key: k });
  async function type() {
    const t = text;
    if (!t) return;
    text = '';
    await send({ k: 'text', text: t });
  }
</script>

<div class="remote">
  <div class="media" role="group" aria-label="Media keys on {name}">
    <button class="btn btn-secondary icon" onclick={() => media('prev')} aria-label="Previous track"><Icon name="prev" size={16} /></button>
    <button class="btn btn-secondary icon" onclick={() => media('play_pause')} aria-label="Play or pause"><Icon name="play" size={16} /></button>
    <button class="btn btn-secondary icon" onclick={() => media('next')} aria-label="Next track"><Icon name="next" size={16} /></button>
    <span class="gap"></span>
    <button class="btn btn-secondary icon" onclick={() => media('vol_down')} aria-label="Volume down"><Icon name="voldown" size={16} /></button>
    <button class="btn btn-secondary icon" onclick={() => media('mute')} aria-label="Mute"><Icon name="mute" size={16} /></button>
    <button class="btn btn-secondary icon" onclick={() => media('vol_up')} aria-label="Volume up"><Icon name="volup" size={16} /></button>
  </div>

  <div
    class="pad"
    role="application"
    aria-label="Touchpad for {name}. Drag to move the pointer, tap to click."
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onwheel={wheel}
    oncontextmenu={(e) => e.preventDefault()}
  >
    <span>Drag to move · tap to click · scroll with the wheel</span>
  </div>

  <div class="buttons">
    <button class="btn btn-secondary" onclick={() => send({ k: 'click', button: 'left' })}>Left click</button>
    <button class="btn btn-secondary" onclick={() => send({ k: 'click', button: 'double' })}>Double click</button>
    <button class="btn btn-secondary" onclick={() => send({ k: 'click', button: 'right' })}>Right click</button>
    <span class="gap"></span>
    <button class="btn btn-secondary" onclick={() => key('left')} aria-label="Left arrow (previous slide)">←</button>
    <button class="btn btn-secondary" onclick={() => key('right')} aria-label="Right arrow (next slide)">→</button>
    <button class="btn btn-secondary" onclick={() => key('esc')}>Esc</button>
    <button class="btn btn-secondary" onclick={() => key('enter')}>Enter</button>
  </div>

  <form
    class="type"
    onsubmit={(e) => {
      e.preventDefault();
      type();
    }}
  >
    <input class="field" bind:value={text} placeholder="Type on {name}" aria-label="Text to type on {name}" maxlength="500" autocomplete="off" />
    <button class="btn btn-primary" disabled={!text}>Type</button>
  </form>
</div>

<style>
  .remote {
    display: grid;
    gap: 12px;
    padding: 4px 20px 20px;
  }
  .media,
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .gap {
    flex: 1;
  }
  .icon {
    width: 40px;
    padding: 0;
    justify-content: center;
  }
  .pad {
    height: 180px;
    border-radius: var(--radius-card);
    border: 1px dashed var(--control);
    background: var(--raised);
    display: grid;
    place-items: center;
    color: var(--muted);
    font-size: 13px;
    touch-action: none;
    cursor: crosshair;
  }
  .pad:active {
    border-color: var(--hue);
  }
  .type {
    display: flex;
    gap: 8px;
  }
</style>
