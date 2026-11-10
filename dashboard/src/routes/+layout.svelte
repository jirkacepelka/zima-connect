<script lang="ts">
  import '../app.css';
</script>

<div class="min-h-screen bg-zima-darkest text-zinc-100 flex flex-col">
  <!-- Top Navigation Bar -->
  <header class="h-14 bg-zima-darker border-b border-zima-border px-6 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div class="w-7 h-7 rounded-lg bg-zima-accent flex items-center justify-center font-bold text-white text-sm">
        Z
      </div>
      <span class="font-semibold text-sm tracking-wide text-zinc-100">Zima Connect Server</span>
      <span class="text-[11px] font-mono px-2 py-0.5 rounded bg-zinc-800 text-zinc-400 border border-zinc-700/60">ZimaOS App</span>
    </div>

    <div class="flex items-center gap-4 text-xs">
      <div class="flex items-center gap-1.5 text-emerald-400">
        <span class="w-2 h-2 rounded-full bg-emerald-400"></span>
        <span>Gateway Healthy</span>
      </div>
      <span class="text-zinc-600">|</span>
      <span class="text-zinc-400 font-mono">Port: 9120</span>
    </div>
  </header>

  <main class="flex-1 max-w-7xl w-full mx-auto p-8">
    <slot />
  </main>
</div>
