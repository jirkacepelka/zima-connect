<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import { api, errText } from '$lib/api';

  let { peerId, name, say }: { peerId: string; name: string; say: (t: string, err?: boolean) => void } = $props();

  type Chunk = { seq: number; key: boolean; ts: number; data: string };
  type Video = { frames?: Chunk[]; codec?: string; width?: number; height?: number };
  let problem = $state('');
  let control = $state(false);
  let ratio = $state(9 / 19.5);
  let canvas: HTMLCanvasElement | undefined = $state();
  let fps = $state(0);

  // H.264 from the phone, decoded by the system (WebCodecs) and drawn at once.
  // The next request goes out as soon as the last one is answered, so the video keeps flowing.
  onMount(() => {
    let alive = true;
    let decoder: VideoDecoder | null = null;
    let cfg = '';
    let after = -1;
    let shown = 0;
    const timer = setInterval(() => {
      fps = shown;
      shown = 0;
    }, 1000);
    const bytes = (b64: string) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    function decoderFor(codec: string) {
      if (decoder && decoder.state !== 'closed' && cfg === codec) return decoder;
      decoder?.close();
      cfg = codec;
      decoder = new VideoDecoder({
        output: (f) => {
          if (canvas) {
            if (canvas.width !== f.displayWidth) canvas.width = f.displayWidth;
            if (canvas.height !== f.displayHeight) canvas.height = f.displayHeight;
            canvas.getContext('2d')!.drawImage(f, 0, 0);
          }
          f.close();
          shown++;
        },
        error: (e) => {
          problem = 'Could not play the video: ' + e.message;
          after = -1;
          decoder = null;
        }
      });
      decoder.configure({ codec, optimizeForLatency: true, avc: { format: 'annexb' } } as VideoDecoderConfig);
      return decoder;
    }
    (async () => {
      if (typeof VideoDecoder === 'undefined') {
        problem = 'This computer cannot play video here (WebCodecs is missing). Update the app or Windows.';
        return;
      }
      try {
        const s = (await api.screenOp(peerId, { op: 'status' })) as { control?: boolean };
        control = !!s.control;
      } catch {
        /* the video request below shows the reason */
      }
      while (alive) {
        try {
          const v = (await api.screenOp(peerId, { op: 'video', after })) as Video;
          problem = '';
          if (v.width && v.height) ratio = v.width / v.height;
          for (const f of v.frames ?? []) {
            const d = decoderFor(v.codec ?? 'avc1.42001f');
            if (d.decodeQueueSize > 6 && !f.key) continue; // too far behind: wait for the next key frame
            d.decode(new EncodedVideoChunk({ type: f.key ? 'key' : 'delta', timestamp: f.ts, data: bytes(f.data) }));
            after = f.seq;
          }
        } catch (e) {
          problem = errText(e);
          after = -1;
          await new Promise((r) => setTimeout(r, 1500));
        }
      }
    })();
    return () => {
      alive = false;
      clearInterval(timer);
      decoder?.close();
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
    const r = canvas!.getBoundingClientRect();
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
  {#if problem && !fps}
    <div class="note"><Icon name="alert" size={18} /> {problem}</div>
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <canvas bind:this={canvas} aria-label="Screen of {name}" class:live={control} style="aspect-ratio:{ratio}" onpointerdown={pdown} onpointerup={pup} onwheel={wheel}></canvas>
    {#if problem}<div class="note small">{problem}</div>{/if}
    <div class="keys">
      <button class="btn btn-secondary" disabled={!control} onclick={() => send({ op: 'key', name: 'back' })}>Back</button>
      <button class="btn btn-secondary" disabled={!control} onclick={() => send({ op: 'key', name: 'home' })}>Home</button>
      <button class="btn btn-secondary" disabled={!control} onclick={() => send({ op: 'key', name: 'recents' })}>Recent apps</button>
      <button class="btn btn-secondary" disabled={!control} onclick={() => send({ op: 'key', name: 'notifications' })}>Notifications</button>
      <span class="sub num fps">{fps} fps</span>
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
  canvas {
    height: min(66vh, 640px);
    max-width: 100%;
    border-radius: 18px;
    background: var(--raised);
    box-shadow: 0 0 0 1px var(--border), 0 12px 32px -16px rgb(0 0 0 / 0.5);
    user-select: none;
    touch-action: none;
  }
  canvas.live {
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
    align-items: center;
  }
  .fps {
    margin-left: 6px;
  }
</style>
