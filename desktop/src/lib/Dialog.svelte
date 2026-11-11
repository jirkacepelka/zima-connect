<script lang="ts">
  import { onMount, type Snippet } from 'svelte';

  import Icon from '$lib/Icon.svelte';

  let { title, onclose, onback, wide = false, children }: { title: string; onclose: () => void; onback?: () => void; wide?: boolean; children: Snippet } = $props();

  let box: HTMLDivElement;
  let previous: HTMLElement | null = null;

  const focusable = () =>
    Array.from(box.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), textarea:not([disabled]), [href], [tabindex]:not([tabindex="-1"])'));

  onMount(() => {
    previous = document.activeElement as HTMLElement | null;
    (focusable().find((el) => el.matches('input, textarea')) ?? focusable()[0])?.focus();
    return () => previous?.focus();
  });

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
      return;
    }
    if (e.key !== 'Tab') return;
    const f = focusable();
    if (!f.length) return;
    const first = f[0];
    const last = f[f.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet" role="dialog" aria-modal="true" aria-label={title} class:wide tabindex="-1" bind:this={box} onkeydown={key}>
    <div class="top">
      {#if onback}<button class="back" onclick={onback} aria-label="Back" title="Back"><Icon name="back" size={16} /></button>{/if}
      <h3>{title}</h3>
    </div>
    {@render children()}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    display: grid;
    place-items: center;
    z-index: 50;
    padding: 20px;
  }
  .sheet.wide {
    width: min(760px, 100%);
    min-height: 380px;
  }
  .sheet {
    width: min(460px, 100%);
    max-height: calc(100vh - 40px);
    overflow: auto;
    background: var(--surface);
    border-radius: var(--radius-card);
    box-shadow: var(--float);
    padding: 22px;
    user-select: text;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 12px;
  }
  .back {
    width: 28px;
    height: 28px;
    margin-left: -6px;
    display: grid;
    place-items: center;
    border-radius: 6px;
    color: var(--muted);
  }
  .back:hover {
    background: var(--raised);
    color: var(--text);
  }
  h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .sheet :global(.lead) {
    color: var(--muted);
    margin: 0 0 14px;
  }
  .sheet :global(.actions) {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
  .sheet :global(.codebox) {
    font-family: var(--mono);
    font-size: 12px;
    word-break: break-all;
    background: var(--raised);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
    max-height: 96px;
    overflow: auto;
  }
  .sheet :global(.qr) {
    display: grid;
    place-items: center;
    margin: 12px 0 0;
  }
  .sheet :global(.seg) {
    display: flex;
    gap: 2px;
    padding: 3px;
    background: var(--active);
    border-radius: 8px;
    margin-bottom: 14px;
  }
  .sheet :global(.seg button) {
    flex: 1;
    height: 30px;
    border-radius: 6px;
    color: var(--muted);
    font-weight: 500;
  }
  .sheet :global(.seg button.on) {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12);
  }
  .sheet :global(.stack) {
    display: grid;
    gap: 10px;
  }
</style>
