<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let selectedDeviceName: string = 'Pixel 8 Pro';
  export let isOnline: boolean = true;

  const dispatch = createEventDispatcher();

  function triggerAction(action: string) {
    dispatch('action', action);
  }
</script>

<div class="space-y-3">
  <div class="flex items-center justify-between">
    <h2 class="text-xs font-semibold uppercase tracking-wider text-zinc-400">
      Quick Actions • <span class="text-zinc-200">{selectedDeviceName}</span>
    </h2>
    <span class="text-[11px] font-mono px-2 py-0.5 rounded border {
      isOnline ? 'border-emerald-500/30 text-emerald-400 bg-emerald-500/10' : 'border-amber-500/30 text-amber-400 bg-amber-500/10'
    }">
      {isOnline ? 'Direct P2P Ready' : 'Store & Forward Mode (Offline)'}
    </span>
  </div>

  <div class="grid grid-cols-3 gap-3">
    <!-- Clipboard Sync -->
    <button
      on:click={() => triggerAction('clipboard')}
      class="p-4 rounded-xl bg-zima-card border border-zima-border hover:border-zinc-700 hover:bg-zinc-800/40 text-left transition-all group flex flex-col justify-between h-28"
    >
      <div class="flex items-center justify-between w-full">
        <div class="w-8 h-8 rounded-lg bg-blue-500/10 text-blue-400 flex items-center justify-center">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"></path>
            <rect x="8" y="2" width="8" height="4" rx="1" ry="1"></rect>
          </svg>
        </div>
        <span class="text-[10px] font-mono text-zinc-400">Auto-Sync</span>
      </div>
      <div>
        <div class="text-xs font-semibold text-zinc-100 group-hover:text-blue-400 transition-colors">Clipboard</div>
        <div class="text-[11px] text-zinc-400">Send or push text</div>
      </div>
    </button>

    <!-- Send File -->
    <button
      on:click={() => triggerAction('file')}
      class="p-4 rounded-xl bg-zima-card border border-zima-border hover:border-zinc-700 hover:bg-zinc-800/40 text-left transition-all group flex flex-col justify-between h-28"
    >
      <div class="flex items-center justify-between w-full">
        <div class="w-8 h-8 rounded-lg bg-emerald-500/10 text-emerald-400 flex items-center justify-center">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"></path>
            <polyline points="13 2 13 9 20 9"></polyline>
          </svg>
        </div>
        <span class="text-[10px] font-mono text-zinc-400">Max 2GB</span>
      </div>
      <div>
        <div class="text-xs font-semibold text-zinc-100 group-hover:text-emerald-400 transition-colors">Send File</div>
        <div class="text-[11px] text-zinc-400">{isOnline ? 'Fast transfer' : 'Stage offline (48h)'}</div>
      </div>
    </button>

    <!-- Notification Mirror -->
    <button
      on:click={() => triggerAction('notifications')}
      class="p-4 rounded-xl bg-zima-card border border-zima-border hover:border-zinc-700 hover:bg-zinc-800/40 text-left transition-all group flex flex-col justify-between h-28"
    >
      <div class="flex items-center justify-between w-full">
        <div class="w-8 h-8 rounded-lg bg-purple-500/10 text-purple-400 flex items-center justify-center">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"></path>
            <path d="M13.73 21a2 2 0 0 1-3.46 0"></path>
          </svg>
        </div>
        <span class="text-[10px] font-mono text-emerald-400">Active</span>
      </div>
      <div>
        <div class="text-xs font-semibold text-zinc-100 group-hover:text-purple-400 transition-colors">Notifications</div>
        <div class="text-[11px] text-zinc-400">Mirrored with reply</div>
      </div>
    </button>

    <!-- Media Controls -->
    <button
      on:click={() => triggerAction('media')}
      class="p-4 rounded-xl bg-zima-card border border-zima-border hover:border-zinc-700 hover:bg-zinc-800/40 text-left transition-all group flex flex-col justify-between h-28"
    >
      <div class="flex items-center justify-between w-full">
        <div class="w-8 h-8 rounded-lg bg-amber-500/10 text-amber-400 flex items-center justify-center">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polygon points="5 3 19 12 5 21 5 3"></polygon>
          </svg>
        </div>
        <span class="text-[10px] font-mono text-zinc-400">MPRIS</span>
      </div>
      <div>
        <div class="text-xs font-semibold text-zinc-100 group-hover:text-amber-400 transition-colors">Media Control</div>
        <div class="text-[11px] text-zinc-400">Play, pause & volume</div>
      </div>
    </button>

    <!-- Ring / Ping Device -->
    <button
      on:click={() => triggerAction('ring')}
      class="p-4 rounded-xl bg-zima-card border border-zima-border hover:border-zinc-700 hover:bg-zinc-800/40 text-left transition-all group flex flex-col justify-between h-28"
    >
      <div class="flex items-center justify-between w-full">
        <div class="w-8 h-8 rounded-lg bg-zima-accent/10 text-zima-accent flex items-center justify-center">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10"></circle>
            <path d="M12 6v6l4 2"></path>
          </svg>
        </div>
        <span class="text-[10px] font-mono text-zinc-400">Locate</span>
      </div>
      <div>
        <div class="text-xs font-semibold text-zinc-100 group-hover:text-zima-accent transition-colors">Ring Device</div>
        <div class="text-[11px] text-zinc-400">Play sound alert</div>
      </div>
    </button>

    <!-- Remote Commands / Terminal -->
    <button
      on:click={() => triggerAction('commands')}
      class="p-4 rounded-xl bg-zima-card border border-zima-border hover:border-zinc-700 hover:bg-zinc-800/40 text-left transition-all group flex flex-col justify-between h-28"
    >
      <div class="flex items-center justify-between w-full">
        <div class="w-8 h-8 rounded-lg bg-cyan-500/10 text-cyan-400 flex items-center justify-center">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="4 17 10 11 4 5"></polyline>
            <line x1="12" y1="19" x2="20" y2="19"></line>
          </svg>
        </div>
        <span class="text-[10px] font-mono text-zinc-400">Custom</span>
      </div>
      <div>
        <div class="text-xs font-semibold text-zinc-100 group-hover:text-cyan-400 transition-colors">Run Commands</div>
        <div class="text-[11px] text-zinc-400">Lock, shutdown, script</div>
      </div>
    </button>
  </div>
</div>
