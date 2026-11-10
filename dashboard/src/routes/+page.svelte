<script lang="ts">
  import { onMount } from 'svelte';

  let stats = {
    total_devices: 3,
    online_devices: 2,
    total_messages: 148,
    pending_messages: 2,
    storage: {
      used_bytes: 1420000000,
      max_bytes: 10737418240,
      file_count: 3,
      used_pct: 13.2
    },
    server_name: "ZimaOS Hub",
    version: "1.0.0"
  };

  let devices = [
    {
      id: "dev-pixel8-1234",
      name: "Pixel 8 Pro",
      platform: "android",
      is_online: true,
      last_seen: "Just now",
      paired_at: "2026-11-01"
    },
    {
      id: "dev-work-9988",
      name: "Workstation PC",
      platform: "windows",
      is_online: true,
      last_seen: "Just now",
      paired_at: "2026-11-02"
    },
    {
      id: "dev-thinkpad-5678",
      name: "ThinkPad Laptop",
      platform: "linux",
      is_online: false,
      last_seen: "2 hours ago",
      paired_at: "2026-11-05"
    }
  ];

  let pendingQueue = [
    {
      from: "Pixel 8 Pro",
      to: "ThinkPad Laptop",
      type: "file.transfer",
      name: "financial_report_q3.pdf",
      size: "14.2 MB",
      expires_in: "46h 12m"
    },
    {
      from: "Workstation PC",
      to: "ThinkPad Laptop",
      type: "clipboard.sync",
      name: "Text payload (240 chars)",
      size: "240 B",
      expires_in: "47h 50m"
    }
  ];

  onMount(async () => {
    try {
      const res = await fetch('/api/v1/admin/stats');
      if (res.ok) {
        stats = await res.json();
      }
    } catch (e) {
      // Keep mock default for offline preview
    }
  });

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }
</script>

<div class="space-y-8">
  <!-- Page Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-xl font-bold text-zinc-100">Server Overview</h1>
      <p class="text-xs text-zinc-400 mt-1">Status of your self-hosted Zima Connect relay on ZimaOS</p>
    </div>
    <div class="flex items-center gap-3">
      <span class="text-xs text-zinc-400 font-mono">SQLite WAL Database: Ready</span>
    </div>
  </div>

  <!-- Key Metrics Grid -->
  <div class="grid grid-cols-4 gap-4">
    <div class="p-5 rounded-xl bg-zima-card border border-zima-border">
      <div class="text-xs font-medium text-zinc-400">Connected Devices</div>
      <div class="text-2xl font-bold text-zinc-100 mt-2 flex items-baseline gap-2">
        <span>{stats.online_devices}</span>
        <span class="text-xs font-normal text-zinc-400">/ {stats.total_devices} paired</span>
      </div>
      <div class="text-[11px] text-emerald-400 mt-2 flex items-center gap-1">
        <span class="w-1.5 h-1.5 rounded-full bg-emerald-400"></span>
        <span>WebSocket listeners active</span>
      </div>
    </div>

    <div class="p-5 rounded-xl bg-zima-card border border-zima-border">
      <div class="text-xs font-medium text-zinc-400">Offline Store-and-Forward</div>
      <div class="text-2xl font-bold text-zinc-100 mt-2">
        {stats.pending_messages} <span class="text-xs font-normal text-zinc-400">pending</span>
      </div>
      <div class="text-[11px] text-amber-400 mt-2">
        Held for offline devices (TTL: 48h)
      </div>
    </div>

    <div class="p-5 rounded-xl bg-zima-card border border-zima-border">
      <div class="text-xs font-medium text-zinc-400">Staging File Storage</div>
      <div class="text-2xl font-bold text-zinc-100 mt-2">
        {formatBytes(stats.storage.used_bytes)}
      </div>
      <div class="text-[11px] text-zinc-400 mt-2">
        {stats.storage.used_pct}% of {formatBytes(stats.storage.max_bytes)} quota
      </div>
    </div>

    <div class="p-5 rounded-xl bg-zima-card border border-zima-border">
      <div class="text-xs font-medium text-zinc-400">Security & Relay</div>
      <div class="text-2xl font-bold text-zinc-100 mt-2 flex items-center gap-2">
        <span class="text-emerald-400">E2E</span>
        <span class="text-xs font-normal text-zinc-400">Zero-knowledge</span>
      </div>
      <div class="text-[11px] text-zinc-400 mt-2">
        Server cannot decrypt envelopes
      </div>
    </div>
  </div>

  <!-- Two Column Layout: Devices Table & Offline Queue -->
  <div class="grid grid-cols-3 gap-6">
    <!-- Paired Devices Table -->
    <div class="col-span-2 space-y-3">
      <h2 class="text-sm font-semibold uppercase tracking-wider text-zinc-400">Paired Devices</h2>

      <div class="rounded-xl border border-zima-border bg-zima-card overflow-hidden">
        <table class="w-full text-left text-xs">
          <thead class="bg-zinc-900/80 border-b border-zima-border text-zinc-400 font-medium">
            <tr>
              <th class="py-3 px-4">Device</th>
              <th class="py-3 px-4">Platform</th>
              <th class="py-3 px-4">Status</th>
              <th class="py-3 px-4">Last Active</th>
              <th class="py-3 px-4 text-right">Actions</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-zinc-800/60">
            {#each devices as dev}
              <tr class="hover:bg-zinc-800/30 transition-colors">
                <td class="py-3.5 px-4 font-medium text-zinc-200">
                  {dev.name}
                  <div class="text-[10px] font-mono text-zinc-400">{dev.id}</div>
                </td>
                <td class="py-3.5 px-4 uppercase text-[11px] font-mono text-zinc-300">
                  {dev.platform}
                </td>
                <td class="py-3.5 px-4">
                  <span class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] {
                    dev.is_online ? 'bg-emerald-500/10 text-emerald-400' : 'bg-zinc-800 text-zinc-400'
                  }">
                    <span class="w-1.5 h-1.5 rounded-full {dev.is_online ? 'bg-emerald-400' : 'bg-zinc-600'}"></span>
                    {dev.is_online ? 'Online' : 'Offline'}
                  </span>
                </td>
                <td class="py-3.5 px-4 text-zinc-400 font-mono text-[11px]">
                  {dev.last_seen}
                </td>
                <td class="py-3.5 px-4 text-right">
                  <button class="text-xs text-red-400 hover:text-red-300 transition-colors">
                    Revoke
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>

    <!-- Active Store-and-Forward Queue -->
    <div class="space-y-3">
      <h2 class="text-sm font-semibold uppercase tracking-wider text-zinc-400">Offline Queue</h2>

      <div class="rounded-xl border border-zima-border bg-zima-card p-4 space-y-3">
        <p class="text-xs text-zinc-400 leading-relaxed">
          Envelopes and files waiting in server staging. These are delivered immediately once the recipient connects.
        </p>

        <div class="space-y-2 pt-1">
          {#each pendingQueue as item}
            <div class="p-3 rounded-lg bg-zinc-900 border border-zinc-800/80 space-y-1.5">
              <div class="flex items-center justify-between text-xs">
                <span class="font-medium text-zinc-200 truncate">{item.name}</span>
                <span class="font-mono text-[10px] text-amber-400">{item.expires_in} left</span>
              </div>
              <div class="text-[11px] text-zinc-400 flex items-center justify-between">
                <span>{item.from} → <strong class="text-zinc-300">{item.to}</strong></span>
                <span class="font-mono">{item.size}</span>
              </div>
            </div>
          {/each}
        </div>
      </div>
    </div>
  </div>
</div>
