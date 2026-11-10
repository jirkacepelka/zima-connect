<script lang="ts">
  import { onMount } from 'svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import Icon from '$lib/Icon.svelte';
  import { api, type ShelfItem } from '$lib/api';
  import { ext, isImage, isPdf } from '$lib/devices';

  let { item, big = false }: { item: ShelfItem; big?: boolean } = $props();

  let src = $state<string | null>(null);
  let canvas: HTMLCanvasElement | undefined = $state();
  let failed = $state(false);
  let root: HTMLDivElement;

  const MAX_IMAGE = 30e6;
  const MAX_PDF = 60e6;
  const wantsPreview = $derived((isImage(item.name) && item.size <= MAX_IMAGE) || (isPdf(item.name) && item.size <= MAX_PDF));

  async function load() {
    try {
      const path = await api.shelfCache(item.id, item.name, item.size);
      const url = path.startsWith('data:') ? path : convertFileSrc(path);
      if (isImage(item.name)) src = url;
      else if (isPdf(item.name)) await renderPdf(url);
    } catch {
      failed = true;
    }
  }

  async function renderPdf(url: string) {
    const pdfjs = await import('pdfjs-dist');
    const worker = (await import('pdfjs-dist/build/pdf.worker.min.mjs?url')).default;
    pdfjs.GlobalWorkerOptions.workerSrc = worker;
    const doc = await pdfjs.getDocument({ url }).promise;
    const page = await doc.getPage(1);
    const base = page.getViewport({ scale: 1 });
    const scale = (big ? 720 : 320) / base.width;
    const vp = page.getViewport({ scale });
    // wait for the canvas element to exist
    for (let i = 0; i < 20 && !canvas; i++) await new Promise((r) => setTimeout(r, 25));
    if (!canvas) return;
    canvas.width = vp.width;
    canvas.height = vp.height;
    await page.render({ canvasContext: canvas.getContext('2d')!, viewport: vp }).promise;
    src = 'pdf';
  }

  // Only fetch what is on screen.
  onMount(() => {
    if (!wantsPreview) return;
    const io = new IntersectionObserver(
      (e) => {
        if (e[0].isIntersecting) {
          io.disconnect();
          load();
        }
      },
      { rootMargin: '200px' }
    );
    io.observe(root);
    return () => io.disconnect();
  });
</script>

<div class="thumb" class:big bind:this={root}>
  {#if wantsPreview && isImage(item.name) && src && !failed}
    <img {src} alt="Preview of {item.name}" draggable="false" onerror={() => (failed = true)} />
  {:else if wantsPreview && isPdf(item.name) && !failed}
    <canvas bind:this={canvas} class:ready={src === 'pdf'} aria-label="First page of {item.name}"></canvas>
    {#if src !== 'pdf'}<span class="ph"><Icon name="file" size={26} /></span>{/if}
  {:else}
    <span class="ph file"><Icon name="file" size={26} /><b>{ext(item.name).slice(0, 4) || 'file'}</b></span>
  {/if}
</div>

<style>
  .thumb {
    position: relative;
    aspect-ratio: 4 / 3;
    background: var(--raised);
    border-radius: 6px;
    overflow: hidden;
    display: grid;
    place-items: center;
  }
  .thumb.big {
    aspect-ratio: auto;
    min-height: 260px;
    max-height: 420px;
  }
  img,
  canvas {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .big img,
  .big canvas {
    object-fit: contain;
  }
  canvas:not(.ready) {
    position: absolute;
    opacity: 0;
  }
  .ph {
    color: var(--muted);
    display: grid;
    place-items: center;
    gap: 6px;
  }
  .ph b {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 600;
  }
</style>
