<script lang="ts">
  import { page } from '$app/state';
  import ScreenView from '$lib/ScreenView.svelte';

  const peer = $derived(page.url.searchParams.get('peer') ?? '');
  const name = $derived(page.url.searchParams.get('name') ?? 'Phone');
  let message = $state('');
</script>

<svelte:head><title>{name}</title></svelte:head>

<main>
  {#if peer}
    <ScreenView peerId={peer} {name} say={(t) => (message = t)} />
    {#if message}<div class="msg">{message}</div>{/if}
  {/if}
</main>

<style>
  main {
    min-height: 100vh;
    display: grid;
    align-content: start;
    background: var(--ground);
  }
  .msg {
    text-align: center;
    color: var(--muted);
    padding-bottom: 8px;
    font-size: 13px;
  }
</style>
