<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';

  const appWindow = typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__ 
    ? getCurrentWindow() 
    : null;

  async function minimize() {
    if (appWindow) await appWindow.minimize();
  }

  async function toggleMaximize() {
    if (appWindow) await appWindow.toggleMaximize();
  }

  async function close() {
    if (appWindow) await appWindow.close();
  }
</script>

<header class="app-drag-region h-11 bg-zima-darker border-b border-zima-border flex items-center justify-between px-3 select-none flex-shrink-0">
  <!-- Brand / Title -->
  <div class="flex items-center gap-2.5">
    <div class="w-6 h-6 rounded-md bg-zima-accent flex items-center justify-center font-bold text-xs text-white shadow-sm shadow-red-500/20">
      Z
    </div>
    <span class="text-xs font-semibold tracking-wide text-zinc-200">ZIMA CONNECT</span>
    <span class="text-[10px] uppercase font-mono px-1.5 py-0.5 rounded bg-zinc-800/80 text-zinc-400 border border-zinc-700/50">v0.1</span>
  </div>

  <!-- Traffic Light Window Controls (Linear/macOS style or Windows styled) -->
  <div class="app-no-drag flex items-center gap-1.5">
    <button
      on:click={minimize}
      class="w-7 h-7 rounded flex items-center justify-center text-zinc-400 hover:text-zinc-100 hover:bg-zinc-800/70 transition-colors"
      title="Minimize"
    >
      <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <line x1="5" y1="12" x2="19" y2="12"></line>
      </svg>
    </button>
    <button
      on:click={toggleMaximize}
      class="w-7 h-7 rounded flex items-center justify-center text-zinc-400 hover:text-zinc-100 hover:bg-zinc-800/70 transition-colors"
      title="Maximize"
    >
      <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect>
      </svg>
    </button>
    <button
      on:click={close}
      class="w-7 h-7 rounded flex items-center justify-center text-zinc-400 hover:text-white hover:bg-red-500/80 transition-colors"
      title="Close"
    >
      <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <line x1="18" y1="6" x2="6" y2="18"></line>
        <line x1="6" y1="6" x2="18" y2="18"></line>
      </svg>
    </button>
  </div>
</header>
