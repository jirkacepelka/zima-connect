<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let devices: Array<{
    id: string;
    name: string;
    platform: string;
    is_online: bool;
    last_seen?: string;
    battery?: number;
  }> = [];

  export let selectedDeviceId: string = '';

  const dispatch = createEventDispatcher();

  function selectDevice(id: string) {
    selectedDeviceId = id;
    dispatch('select', id);
  }

  function openPairing() {
    dispatch('open-pairing');
  }
</script>

<aside class="w-64 bg-zima-darker border-r border-zima-border flex flex-col justify-between p-3 select-none flex-shrink-0">
  <div class="space-y-4">
    <!-- Header -->
    <div class="flex items-center justify-between px-2 pt-1">
      <span class="text-[11px] font-semibold uppercase tracking-wider text-zinc-400">Devices</span>
      <button
        on:click={openPairing}
        class="text-xs font-medium text-zima-accent hover:text-zima-accent-hover flex items-center gap-1 transition-colors"
      >
        <span class="text-base leading-none">+</span> Pair
      </button>
    </div>

    <!-- Device List -->
    <div class="space-y-1.5">
      {#each devices as dev}
        <button
          on:click={() => selectDevice(dev.id)}
          class="w-full text-left p-2.5 rounded-lg border transition-all flex items-center justify-between group {
            selectedDeviceId === dev.id
              ? 'bg-zinc-800/80 border-zinc-700/80 shadow-sm'
              : 'bg-zinc-900/40 border-transparent hover:bg-zinc-800/40 hover:border-zinc-800'
          }"
        >
          <div class="flex items-center gap-3 min-w-0">
            <!-- Platform Icon -->
            <div class="w-8 h-8 rounded-md bg-zinc-800 flex items-center justify-center text-zinc-300 flex-shrink-0">
              {#if dev.platform === 'android'}
                <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="5" y="2" width="14" height="20" rx="2" ry="2"></rect>
                  <line x1="12" y1="18" x2="12.01" y2="18"></line>
                </svg>
              {:else if dev.platform === 'linux'}
                <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M4 17l6-6-6-6"></path>
                  <line x1="12" y1="19" x2="20" y2="19"></line>
                </svg>
              {:else}
                <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect>
                  <line x1="8" y1="21" x2="16" y2="21"></line>
                  <line x1="12" y1="17" x2="12" y2="21"></line>
                </svg>
              {/if}
            </div>

            <!-- Device Details -->
            <div class="min-w-0">
              <div class="text-xs font-medium text-zinc-100 truncate">{dev.name}</div>
              <div class="text-[10px] text-zinc-400 flex items-center gap-1.5 mt-0.5">
                <span class="inline-block w-1.5 h-1.5 rounded-full {dev.is_online ? 'bg-emerald-400' : 'bg-zinc-600'}"></span>
                <span>{dev.is_online ? 'Online' : (dev.last_seen || 'Offline')}</span>
              </div>
            </div>
          </div>

          <!-- Battery badge if available -->
          {#if dev.battery !== undefined}
            <div class="text-[10px] font-mono px-1.5 py-0.5 rounded bg-zinc-800/80 text-zinc-400">
              {dev.battery}%
            </div>
          {/if}
        </button>
      {/each}
    </div>
  </div>

  <!-- Bottom Node Info -->
  <div class="p-2.5 rounded-lg bg-zinc-900/60 border border-zinc-800/80 text-[11px] text-zinc-400 space-y-1">
    <div class="flex items-center justify-between">
      <span>ZimaOS Server</span>
      <span class="text-emerald-400 flex items-center gap-1">
        <span class="w-1.5 h-1.5 rounded-full bg-emerald-400"></span> Connected
      </span>
    </div>
    <div class="text-[10px] font-mono text-zinc-500 truncate">192.168.1.100:9120</div>
  </div>
</aside>
