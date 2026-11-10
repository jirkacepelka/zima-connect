<script lang="ts">
  let { name, size = 20 }: { name: string; size?: number } = $props();

  // Simple 24px stroke icons in the spirit of SF Symbols.
  const paths: Record<string, string> = {
    phone: 'M8 2.5h8a2 2 0 0 1 2 2v15a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2v-15a2 2 0 0 1 2-2zM11 18.5h2',
    laptop: 'M5 5.5h14a1 1 0 0 1 1 1V15H4V6.5a1 1 0 0 1 1-1zM2.5 18.5h19',
    desktop: 'M4 4.5h16a1 1 0 0 1 1 1V15a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5.5a1 1 0 0 1 1-1zM9 20h6M12 16v4',
    server: 'M4 5h16v6H4zM4 13h16v6H4zM7.5 8h.01M7.5 16h.01',
    clipboard: 'M9 4.5h6M9.5 3h5a1 1 0 0 1 1 1v1.5h-7V4a1 1 0 0 1 1-1zM8 4.5H6.5a1 1 0 0 0-1 1V20a1 1 0 0 0 1 1h11a1 1 0 0 0 1-1V5.5a1 1 0 0 0-1-1H16',
    file: 'M14 3H7.5a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1V6.5zM14 3v3.5h3.5',
    bell: 'M6 16.5V11a6 6 0 1 1 12 0v5.5l1.5 1.5h-15zM10 20.5a2 2 0 0 0 4 0',
    plus: 'M12 5v14M5 12h14',
    copy: 'M9 9h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H9a1 1 0 0 1-1-1V10a1 1 0 0 1 1-1zM16 9V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3',
    check: 'M5 12.5l4.5 4.5L19 7.5',
    x: 'M6 6l12 12M18 6L6 18',
    trash: 'M4.5 7h15M10 7V4.5h4V7M6.5 7l.8 12.5h9.4L17.5 7',
    link: 'M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1',
    pair: 'M7 10a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM17 20a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM9.5 8.5l5 7',
    send: 'M4 12l16-8-6 16-2.5-6.5z',
    alert: 'M12 8v5M12 16.5h.01M10.3 4.5L3 17.5A1.5 1.5 0 0 0 4.3 19.5h15.4A1.5 1.5 0 0 0 21 17.5L13.7 4.5a1.5 1.5 0 0 0-2.6 0z',
    users: 'M9 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM2.5 20a6.5 6.5 0 0 1 13 0M16 4.3a3.5 3.5 0 0 1 0 6.4M18 14.2a6.5 6.5 0 0 1 3.5 5.8',
    user: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4.5 20.5a7.5 7.5 0 0 1 15 0',
    shield: 'M12 3l7.5 3v5.5c0 4.5-3.2 8-7.5 9.5-4.3-1.5-7.5-5-7.5-9.5V6z',
    server2: 'M5 6h14M5 12h14M5 18h14',
    folder: 'M3.5 6.5a1 1 0 0 1 1-1H10l2 2h7.5a1 1 0 0 1 1 1V18a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1z'
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.7"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name] ?? ''} />
</svg>
