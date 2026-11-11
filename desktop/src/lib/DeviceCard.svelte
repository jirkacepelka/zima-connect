<script lang="ts">
  import Icon from '$lib/Icon.svelte';
  import Mockup from '$lib/Mockup.svelte';
  import type { Contact } from '$lib/api';
  import { PLATFORM, hueFor, kindOf, ownerName } from '$lib/devices';

  let { c, over = false, onopen }: { c: Contact; over?: boolean; onopen: () => void } = $props();

  const hue = $derived(hueFor(c));
  const charging = $derived(!!c.battery && c.battery[1]);
</script>

<button class="dev" class:over class:offline={!c.online} data-device-id={c.id} style="--h:{hue}" onclick={onopen} aria-label="{c.name}, {c.online ? 'connected' : 'offline'}. Open.">
  <div class="art"><Mockup kind={kindOf(c.platform, c.name)} platform={c.platform} {hue} online={c.online} battery={c.battery} size={132} fit="height" /></div>
  <div class="info">
    <div class="name">{c.name}</div>
    <div class="owner"><span class="av">{(c.user_name ?? '·').slice(0, 1).toUpperCase()}</span>{ownerName(c)} <span class="sep">·</span> {PLATFORM[c.platform] ?? c.platform}</div>
    <div class="state num">
      <span class="dot" class:on={c.online}></span>{c.online ? `Connected${c.rtt_ms !== null ? `, ${c.rtt_ms} ms` : ''}` : 'Offline'}
      {#if c.battery}
        <span class="bat" class:charging title={charging ? 'Charging' : 'Battery'}>
          <Icon name="battery" size={14} />{c.battery[0]} %{#if charging}<i class="zap">⚡</i>{/if}
        </span>
      {/if}
    </div>
  </div>
  {#if over}<div class="drop-hint">Drop to send to {c.name}</div>{/if}
</button>

<style>
  .dev {
    position: relative;
    display: flex;
    flex-direction: column;
    text-align: left;
    width: 100%;
    background: none;
    border-radius: 16px;
    transition: transform 0.15s;
  }
  .dev {
    padding: 14px 10px 16px;
  }
  .dev:hover {
    transform: translateY(-2px);
    background: var(--surface);
  }
  .dev.over {
    box-shadow: 0 0 0 2px var(--hue);
  }
  .art {
    display: grid;
    place-items: center;
    padding: 0;
    height: 150px;
  }
  .offline .art {
    opacity: 0.55;
  }
  .info {
    padding: 10px 4px 0;
    text-align: center;
    justify-items: center;
    display: grid;
    gap: 3px;
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .owner,
  .state {
    color: var(--muted);
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sep {
    opacity: 0.6;
  }
  .av {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: hsl(var(--h) 55% 90%);
    color: hsl(var(--h) 55% 28%);
    display: inline-grid;
    place-items: center;
    font-size: 10px;
    font-weight: 600;
    flex: none;
  }
  @media (prefers-color-scheme: dark) {
    .av {
      background: hsl(var(--h) 35% 24%);
      color: hsl(var(--h) 70% 85%);
    }
  }
  .bat {
    margin-left: 6px;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--text);
  }
  .zap {
    font-style: normal;
    animation: zap 1.4s ease-in-out infinite;
  }
  @keyframes zap {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }
  .drop-hint {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    border-radius: 16px;
    background: color-mix(in srgb, var(--ground) 80%, transparent);
    font-weight: 600;
    text-align: center;
    padding: 12px;
  }
  @media (prefers-reduced-motion: reduce) {
    .zap {
      animation: none;
    }
  }
</style>
