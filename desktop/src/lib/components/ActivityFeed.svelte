<script lang="ts">
  export let activities: Array<{
    id: string;
    timestamp: string;
    icon: string;
    title: string;
    description: string;
    status: string;
  }> = [];
</script>

<div class="space-y-3">
  <div class="flex items-center justify-between">
    <h2 class="text-xs font-semibold uppercase tracking-wider text-zinc-400">Activity Log</h2>
    <span class="text-[11px] text-zinc-400 hover:text-zinc-200 cursor-pointer">Clear</span>
  </div>

  <div class="space-y-2 max-h-56 overflow-y-auto pr-1">
    {#each activities as item}
      <div class="p-3 rounded-lg bg-zima-card border border-zima-border/70 flex items-center justify-between">
        <div class="flex items-center gap-3 min-w-0">
          <div class="w-7 h-7 rounded-md bg-zinc-800 flex items-center justify-center text-zinc-300 flex-shrink-0">
            {#if item.icon === 'file'}
              <svg class="w-3.5 h-3.5 text-emerald-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"></path>
              </svg>
            {:else if item.icon === 'clipboard'}
              <svg class="w-3.5 h-3.5 text-blue-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"></path>
              </svg>
            {:else}
              <svg class="w-3.5 h-3.5 text-purple-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"></path>
              </svg>
            {/if}
          </div>

          <div class="min-w-0">
            <div class="text-xs font-medium text-zinc-200 truncate">{item.title}</div>
            <div class="text-[11px] text-zinc-400 truncate">{item.description}</div>
          </div>
        </div>

        <div class="text-right flex-shrink-0 ml-3">
          <div class="text-[10px] text-zinc-400 font-mono">{item.timestamp}</div>
          <span class="inline-block px-1.5 py-0.5 rounded text-[9px] uppercase font-mono mt-0.5 {
            item.status === 'success'
              ? 'text-emerald-400 bg-emerald-500/10'
              : 'text-amber-400 bg-amber-500/10'
          }">
            {item.status}
          </span>
        </div>
      </div>
    {/each}
  </div>
</div>
