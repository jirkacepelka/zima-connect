<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import { api, errText } from '$lib/api';

  let { peerId, name, say }: { peerId: string; name: string; say: (t: string, err?: boolean) => void } = $props();

  type Frame = { jpg?: string; width?: number; height?: number; wait?: boolean };
  let src = $state('');
  let ratio = $state(9 / 19.5);
  let problem = $state('');
  let control = $state(false);
  let img: HTMLImageElement | undefined = $state();

  // Ask for pictures one after the other: the next one is requested when the last arrived.
  onMount(() => {
    let alive = true;
    (async () => {
      try {
        const s = (await api.screenOp(peerId, { op: 'status' })) as { control?: boolean };
        control = !!s.control;
      } catch {
        /* the frame request below shows the reason */
      }
      while (alive) {
        try {
          const f = (await api.screenOp(peerId, { op: 'frame', max_width: null })) as Frame;
          problem = '';
          if (f.jpg) {
            src = 'data:image/jpeg;base64,' + f.jpg;
            if (f.width && f.height) ratio = f.width / f.height;
          } else {
            await new Promise((r) => setTimeout(r, 150));
          }
        } catch (e) {
          problem = errText(e);
          await new Promise((r) => setTimeout(r, 1500));
        }
      }
    })();
    return () => {
      alive = false;
    };
  });

  async function send(op: Parameters<typeof api.screenOp>[1]) {
    try {
      await api.screenOp(peerId, op);
    } catch (e) {
      say(errText(e), true);
    }
  }

  // Click = tap, press and hold = long press, drag = swipe.
  let down: { x: number; y: number; t: number } | null = null;
  const at = (e: PointerEvent) => {
    const r = img!.getBoundingClientRect();
    return { x: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)), y: Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)) };
  };
  function pdown(e: PointerEvent) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    down = { ...at(e), t: performance.now() };
  }
  function pup(e: PointerEvent) {
    if (!down) return;
    const a = down;
    const b = at(e);
    down = null;
    const ms = performance.now() - a.t;
    if (Math.hypot(b.x - a.x, b.y - a.y) > 0.015) send({ op: 'swipe', x1: a.x, y1: a.y, x2: b.x, y2: b.y, ms: Math.round(Math.min(Math.max(ms, 80), 1200)) });
    else if (ms > 500) send({ op: 'hold', x: a.x, y: a.y });
    else send({ op: 'tap', x: a.x, y: a.y });
  }
  function wheel(e: WheelEvent) {
    e.preventDefault();
    const d = Math.sign(e.deltaY) * 0.3;
    send({ op: 'swipe', x1: 0.5, y1: 0.5 + d / 2, x2: 0.5, y2: 0.5 - d / 2, ms: 220 });
  }
</script>

<div class="wrap">
  {#if problem && !src}
    <div class="note"><Icon name="alert" size={18} /> {problem}</div>
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <img
      bind:this={img}
      {src}
      alt="Screen of {name}"
      class:live={control}
      style="aspect-ratio:{ratio}"
      draggable="false"
      onpointerdown={pdown}
      onpointerup={pup}
      onwheel={wheel}
    />
    {#if problem}<div class="note small">{problem}</div>{/if}
    <div class="keys">
      <button class="btn btn-secondary" disabled={!control} onclick={() => send({ op: 'key', name: 'back' })}>Back</button>
      <button class="btn btn-secondary" disabled={!control} onclick={() => send({ op: 'key', name: 'home' })}>Home</button>
      <button class="btn btn-secondary" disabled={!control} onclick={() => send({ op: 'key', name: 'recents' })}>Recent apps</button>
      <button class="btn btn-secondary" disabled={!control} onclick={() => send({ op: 'key', name: 'notifications' })}>Notifications</button>
    </div>
    {#if !control}<div class="sub">Viewing only. To touch it from here, turn on Zima Connect in the phone's Accessibility settings.</div>{/if}
  {/if}
</div>

<style>
  .wrap {
    display: grid;
    justify-items: center;
    gap: 12px;
  }
  img {
    height: min(66vh, 640px);
    max-width: 100%;
    border-radius: 18px;
    background: var(--raised);
    box-shadow: 0 0 0 1px var(--border), 0 12px 32px -16px rgb(0 0 0 / 0.5);
    user-select: none;
    touch-action: none;
  }
  img.live {
    cursor: pointer;
  }
  .note {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 28px 10px;
    color: var(--muted);
    text-align: center;
  }
  .note.small {
    padding: 0;
    font-size: 13px;
  }
  .keys {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }
</style>
