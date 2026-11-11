<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import { api } from '$lib/api';
  import { answer, decline, hangUp, phone, toggleMute } from '$lib/phone.svelte';

  let now = $state(Date.now());
  onMount(() => {
    const t = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(t);
  });
  const clock = $derived.by(() => {
    const s = Math.max(0, Math.floor((now - phone.since) / 1000));
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
  });
  const who = $derived(phone.name || phone.number || 'Unknown number');
</script>

{#if phone.line !== 'idle'}
  <div class="call" role="alertdialog" aria-label="Phone call">
    <div class="top">
      <div class="who">
        <span class="av"><Icon name="call" size={18} /></span>
        <div>
          <b>{who}</b>
          <div class="sub num">
            {#if phone.line === 'ringing'}Calling {phone.deviceName}
            {:else if phone.line === 'dialing'}Calling…
            {:else}{clock} · on {phone.deviceName}{/if}
          </div>
        </div>
      </div>
      <div class="acts">
        {#if phone.line === 'ringing'}
          <button class="round no" onclick={decline} aria-label="Decline"><Icon name="hangup" size={20} /></button>
          <button class="round yes" onclick={answer} aria-label="Answer"><Icon name="call" size={20} /></button>
        {:else if phone.line === 'dialing'}
          <button class="round no" onclick={hangUp} aria-label="Cancel call"><Icon name="hangup" size={20} /></button>
        {:else}
          <button class="round mute" class:on={phone.muted} onclick={toggleMute} aria-pressed={phone.muted} aria-label={phone.muted ? 'Unmute' : 'Mute'}>
            <Icon name={phone.muted ? 'mute' : 'volup'} size={18} />
          </button>
          <button class="round no" onclick={hangUp} aria-label="Hang up"><Icon name="hangup" size={20} /></button>
        {/if}
      </div>
    </div>
    {#if phone.error}
      <div class="note err">{phone.error}</div>
    {:else if phone.line !== 'ringing'}
      <div class="note sub">
        You talk on the phone.
        <button class="link" onclick={() => api.bluetoothSettings().catch(() => {})}>Talk on this computer instead</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .call {
    position: fixed;
    right: 24px;
    bottom: 24px;
    z-index: 70;
    display: grid;
    gap: 10px;
    padding: 14px 16px 12px 18px;
    border-radius: 18px;
    background: var(--surface);
    box-shadow: var(--float);
    animation: rise 0.2s ease-out;
    max-width: 380px;
  }
  @keyframes rise {
    from {
      transform: translateY(10px);
      opacity: 0;
    }
  }
  .top {
    display: flex;
    align-items: center;
    gap: 22px;
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
  }
  .acts {
    display: flex;
    gap: 10px;
    margin-left: auto;
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
  .note {
    font-size: 13px;
  }
  .note.err {
    color: #d63b3b;
  }
  @media (prefers-reduced-motion: reduce) {
    .call {
      animation: none;
    }
  }
</style>
