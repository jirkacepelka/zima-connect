<script lang="ts">
  export let serverUrl: string = 'http://localhost:9120';
  export let connected: boolean = true;
  export let activeDevices: number = 2;
  export let txBytes: number = 2400000;
  export let rxBytes: number = 156000;

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }
</script>

<footer class="h-8 bg-zima-darker border-t border-zima-border px-3 flex items-center justify-between text-[11px] font-mono text-zinc-400 select-none flex-shrink-0">
  <div class="flex items-center gap-3">
    <div class="flex items-center gap-1.5" title={serverUrl}>
      <span class="w-1.5 h-1.5 rounded-full {connected ? 'bg-emerald-400' : 'bg-red-400'}"></span>
      <span class="text-zinc-300">{connected ? 'ZimaOS Hub Connected' : 'Disconnected'}</span>
    </div>
    <span class="text-zinc-600">•</span>
    <span class="text-zinc-400">{activeDevices} devices paired</span>
  </div>

  <div class="flex items-center gap-4">
    <div class="flex items-center gap-2">
      <span class="text-zinc-400">↑ {formatBytes(txBytes)}</span>
      <span class="text-zinc-400">↓ {formatBytes(rxBytes)}</span>
    </div>
    <span class="text-zinc-600">•</span>
    <span class="text-emerald-400/90 flex items-center gap-1">
      <svg class="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect>
        <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
      </svg>
      E2E Encrypted
    </span>
  </div>
</footer>
