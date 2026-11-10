<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let open: boolean = false;
  let code: string = '';
  let errorMsg: string = '';

  const dispatch = createEventDispatcher();

  function close() {
    open = false;
    code = '';
    errorMsg = '';
    dispatch('close');
  }

  function handlePair() {
    if (code.trim().length !== 6) {
      errorMsg = 'Please enter a valid 6-digit code';
      return;
    }
    dispatch('confirm', code.trim());
    close();
  }
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm animate-fade-in">
    <div class="w-96 rounded-xl bg-zinc-900 border border-zinc-800 p-6 shadow-2xl space-y-5">
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-semibold text-zinc-100">Pair New Device</h3>
        <button on:click={close} class="text-zinc-400 hover:text-zinc-200 text-lg leading-none">&times;</button>
      </div>

      <p class="text-xs text-zinc-400 leading-relaxed">
        Enter the 6-digit PIN displayed on your Android phone or target device. This verifies the end-to-end encryption key exchange.
      </p>

      <div class="space-y-2">
        <input
          type="text"
          maxlength="6"
          placeholder="000 000"
          bind:value={code}
          class="w-full text-center text-2xl font-mono tracking-widest py-3 px-4 rounded-lg bg-zinc-950 border border-zinc-700 text-zinc-100 focus:outline-none focus:border-zima-accent"
        />
        {#if errorMsg}
          <p class="text-[11px] text-red-400 text-center">{errorMsg}</p>
        {/if}
      </div>

      <div class="flex items-center justify-end gap-2 pt-2">
        <button
          on:click={close}
          class="px-3.5 py-2 rounded-lg text-xs font-medium text-zinc-300 hover:bg-zinc-800 transition-colors"
        >
          Cancel
        </button>
        <button
          on:click={handlePair}
          class="px-4 py-2 rounded-lg text-xs font-medium bg-zima-accent hover:bg-zima-accent-hover text-white transition-colors shadow-sm"
        >
          Pair Device
        </button>
      </div>
    </div>
  </div>
{/if}
