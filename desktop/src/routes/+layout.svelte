<script lang="ts">
  import '../app.css';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import StatusBar from '$lib/components/StatusBar.svelte';
</script>

<div class="h-screen w-screen flex flex-col bg-zima-darkest overflow-hidden text-zinc-100">
  <TitleBar />
  <main class="flex-1 overflow-hidden flex">
    <slot />
  </main>
  <StatusBar />
</div>
