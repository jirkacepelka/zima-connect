<script lang="ts">
  import { onMount } from 'svelte';
  import DeviceSidebar from '$lib/components/DeviceSidebar.svelte';
  import PluginTiles from '$lib/components/PluginTiles.svelte';
  import ActivityFeed from '$lib/components/ActivityFeed.svelte';
  import PairingModal from '$lib/components/PairingModal.svelte';

  // Tauri invoke wrapper with browser fallback
  async function invokeTauri<T>(cmd: string, args: Record<string, any> = {}): Promise<T | null> {
    if (typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__) {
      const { invoke } = await import('@tauri-apps/api/core');
      return await invoke<T>(cmd, args);
    }
    return null;
  }

  let devices = [
    {
      id: 'dev-pixel8-1234',
      name: 'Pixel 8 Pro',
      platform: 'android',
      is_online: true,
      last_seen: 'Just now',
      battery: 84
    },
    {
      id: 'dev-thinkpad-5678',
      name: 'ThinkPad X1',
      platform: 'linux',
      is_online: false,
      last_seen: '2 hours ago',
      battery: 62
    }
  ];

  let selectedDeviceId = 'dev-pixel8-1234';
  let showPairingModal = false;

  let activities = [
    {
      id: '1',
      timestamp: '2 min ago',
      icon: 'file',
      title: 'File sent: presentation_final.pdf',
      description: 'Delivered to Pixel 8 Pro (14.2 MB)',
      status: 'success'
    },
    {
      id: '2',
      timestamp: '15 min ago',
      icon: 'clipboard',
      title: 'Clipboard synchronized',
      description: 'Copied link from Pixel 8 Pro',
      status: 'success'
    },
    {
      id: '3',
      timestamp: '1 hour ago',
      icon: 'bell',
      title: 'Notification mirrored',
      description: 'Slack: Team meeting in 10 minutes',
      status: 'success'
    }
  ];

  $: selectedDevice = devices.find(d => d.id === selectedDeviceId) || devices[0];

  onMount(async () => {
    const fetchedDevices = await invokeTauri<any[]>('get_devices');
    if (fetchedDevices && fetchedDevices.length > 0) {
      devices = fetchedDevices;
      selectedDeviceId = devices[0].id;
    }

    const fetchedActivities = await invokeTauri<any[]>('get_activities');
    if (fetchedActivities && fetchedActivities.length > 0) {
      activities = fetchedActivities;
    }
  });

  async function handleAction(e: CustomEvent<string>) {
    const action = e.detail;
    if (action === 'clipboard') {
      const res = await invokeTauri('send_clipboard', {
        targetDeviceId: selectedDeviceId,
        content: 'Sample clipboard from desktop'
      });
      alert(res || 'Clipboard content pushed to device');
    } else if (action === 'file') {
      alert(`Select file dialog: staging file for ${selectedDevice.name} (with 48h offline hold)`);
    } else if (action === 'ring') {
      await invokeTauri('ring_device', { targetDeviceId: selectedDeviceId });
      alert(`Ping sent to ${selectedDevice.name}`);
    } else {
      alert(`Action '${action}' executed for ${selectedDevice.name}`);
    }
  }

  async function handlePairConfirm(e: CustomEvent<string>) {
    const pin = e.detail;
    await invokeTauri('pair_device', { code: pin });
    alert(`Device successfully paired with PIN ${pin}!`);
  }
</script>

<div class="flex-1 flex overflow-hidden">
  <!-- Sidebar -->
  <DeviceSidebar
    {devices}
    {selectedDeviceId}
    on:select={(e) => selectedDeviceId = e.detail}
    on:open-pairing={() => showPairingModal = true}
  />

  <!-- Main Content Area -->
  <section class="flex-1 overflow-y-auto p-6 space-y-6">
    <!-- Active Device Header Banner -->
    <div class="p-5 rounded-2xl bg-gradient-to-r from-zinc-900 via-zinc-900 to-zinc-900/60 border border-zinc-800 flex items-center justify-between">
      <div class="flex items-center gap-4">
        <div class="w-12 h-12 rounded-xl bg-zinc-800 flex items-center justify-center text-zinc-100 border border-zinc-700/60 shadow-inner">
          {#if selectedDevice.platform === 'android'}
            <svg class="w-6 h-6" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="5" y="2" width="14" height="20" rx="2" ry="2"></rect>
              <line x1="12" y1="18" x2="12.01" y2="18"></line>
            </svg>
          {:else}
            <svg class="w-6 h-6" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect>
              <line x1="8" y1="21" x2="16" y2="21"></line>
            </svg>
          {/if}
        </div>
        <div>
          <div class="flex items-center gap-2">
            <h1 class="text-base font-semibold text-zinc-100">{selectedDevice.name}</h1>
            <span class="w-2 h-2 rounded-full {selectedDevice.is_online ? 'bg-emerald-400' : 'bg-zinc-600'}"></span>
          </div>
          <p class="text-xs text-zinc-400 mt-0.5">
            {selectedDevice.platform.toUpperCase()} • {selectedDevice.is_online ? 'Connected via ZimaOS Hub' : 'Offline (Queue enabled)'}
          </p>
        </div>
      </div>

      <div class="flex items-center gap-2">
        <button
          on:click={() => handleAction(new CustomEvent('action', { detail: 'ring' }))}
          class="px-3.5 py-1.5 rounded-lg text-xs font-medium bg-zinc-800 hover:bg-zinc-700 text-zinc-200 border border-zinc-700 transition-colors flex items-center gap-1.5"
        >
          <svg class="w-3.5 h-3.5 text-zima-accent" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10"></circle>
            <path d="M12 6v6l4 2"></path>
          </svg>
          Find Device
        </button>
      </div>
    </div>

    <!-- Quick Action Plugin Tiles -->
    <PluginTiles
      selectedDeviceName={selectedDevice.name}
      isOnline={selectedDevice.is_online}
      on:action={handleAction}
    />

    <!-- Real-time Activity Feed -->
    <ActivityFeed {activities} />
  </section>
</div>

<!-- Pairing PIN Dialog -->
<PairingModal
  bind:open={showPairingModal}
  on:confirm={handlePairConfirm}
/>
