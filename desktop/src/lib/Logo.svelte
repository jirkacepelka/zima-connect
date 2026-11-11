<script lang="ts">
  let { size = 28 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
  <defs>
    <linearGradient id="zc-logo" x1="8" y1="4" x2="56" y2="60" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#4C8DFF" /><stop offset="1" stop-color="#7B4DFF" /></linearGradient>
  </defs>
  <rect width="64" height="64" rx="16" fill="url(#zc-logo)" />
  <path d="M21 21.5h22L21 42.5h22" fill="none" stroke="#fff" stroke-width="5" stroke-linecap="round" stroke-linejoin="round" />
  <circle cx="21" cy="21.5" r="5.2" fill="#fff" />
  <circle cx="43" cy="42.5" r="5.2" fill="#fff" />
</svg>
