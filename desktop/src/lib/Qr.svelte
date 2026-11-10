<script lang="ts">
  import QRCode from 'qrcode';

  let { text, size = 260 }: { text: string; size?: number } = $props();
  let url = $state('');

  $effect(() => {
    const t = text;
    if (!t) {
      url = '';
      return;
    }
    // Black on white regardless of theme: scanners need the contrast.
    QRCode.toDataURL(t, { margin: 2, width: size * 2, errorCorrectionLevel: 'L' })
      .then((u) => (url = u))
      .catch(() => (url = ''));
  });
</script>

{#if url}
  <img class="qr" src={url} width={size} height={size} alt="QR code with the same code" />
{/if}

<style>
  .qr {
    display: block;
    margin: 0 auto;
    border-radius: 12px;
    background: #fff;
    image-rendering: pixelated;
  }
</style>
