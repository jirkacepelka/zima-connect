<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import { acceptCall, call, declineCall, hangUp, toggleMute } from '$lib/call.svelte';

  let now = $state(Date.now());
  onMount(() => {
    const t = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(t);
  });
  const clock = $derived.by(() => {
    const s = Math.max(0, Math.floor((now - call.since) / 1000));
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
  });
</script>

{#if call.state !== 'idle'}
  <div class="call" role="alertdialog" aria-label="Voice call">
    <div class="who">
      <span class="av">{call.peer.slice(0, 1).toUpperCase()}</span>
      <div>
        <b>{call.peer}</b>
        <div class="sub num">
          {#if call.state === 'incoming'}Incoming call
          {:else if call.state === 'ringing'}Calling…
          {:else if call.state === 'connected'}{clock}
          {:else}{call.note}{/if}
        </div>
      </div>
    </div>
    <div class="acts">
      {#if call.state === 'incoming'}
        <button class="round no" onclick={declineCall} aria-label="Decline"><Icon name="hangup" size={20} /></button>
        <button class="round yes" onclick={acceptCall} aria-label="Answer"><Icon name="call" size={20} /></button>
      {:else if call.state === 'ringing'}
        <button class="round no" onclick={hangUp} aria-label="Cancel call"><Icon name="hangup" size={20} /></button>
      {:else if call.state === 'connected'}
        <button class="round mute" class:on={call.muted} onclick={toggleMute} aria-pressed={call.muted} aria-label={call.muted ? 'Unmute' : 'Mute'}>
          <Icon name={call.muted ? 'mute' : 'volup'} size={18} />
        </button>
        <button class="round no" onclick={hangUp} aria-label="Hang up"><Icon name="hangup" size={20} /></button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .call {
    position: fixed;
    right: 24px;
    bottom: 24px;
    z-index: 70;
    display: flex;
    align-items: center;
    gap: 22px;
    padding: 14px 16px 14px 18px;
    border-radius: 18px;
    background: var(--surface);
    box-shadow: var(--float);
    animation: rise 0.2s ease-out;
  }
  @keyframes rise {
    from {
      transform: translateY(10px);
      opacity: 0;
    }
  }
  .who {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 170px;
  }
  .av {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--active);
    display: grid;
    place-items: center;
    font-weight: 600;
  }
  .acts {
    display: flex;
    gap: 10px;
  }
  .round {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: #fff;
  }
  .round.yes {
    background: #1fa857;
  }
  .round.no {
    background: #d63b3b;
  }
  .round.mute {
    background: var(--active);
    color: var(--text);
  }
  .round.mute.on {
    background: var(--text);
    color: var(--surface);
  }
  @media (prefers-reduced-motion: reduce) {
    .call {
      animation: none;
    }
  }
</style>
