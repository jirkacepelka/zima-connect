<script lang="ts" module>
  import 'devices.css/dist/devices.min.css';

  let counter = 0;

  // Device frames: devices.css (MIT, github.com/picturepan2/devices.css), pure CSS.
  type Model = { cls: string; w: number; h: number; sw: number; sh: number };
  const MODELS: Record<string, Model> = {
    pixel: { cls: 'device-google-pixel-6-pro', w: 404, h: 862, sw: 376, sh: 816 },
    iphone: { cls: 'device-iphone-14-pro', w: 428, h: 868, sw: 390, sh: 830 },
    tablet: { cls: 'device-ipad-pro-2017', w: 560, h: 804, sw: 510, sh: 680 },
    macbook: { cls: 'device-macbook-pro-2018', w: 740, h: 444, sw: 576, sh: 360 },
    surfacebook: { cls: 'device-surface-book', w: 728, h: 424, sw: 540, sh: 360 },
    imac: { cls: 'device-imac-pro', w: 624, h: 484, sw: 576, sh: 324 },
    studio: { cls: 'device-surface-studio', w: 640, h: 506, sw: 600, sh: 400 }
  };
</script>

<script lang="ts">
  let {
    kind,
    platform = '',
    hue = 220,
    online = true,
    battery = null,
    size = 120,
    fit = 'long'
  }: {
    kind: 'phone' | 'tablet' | 'laptop' | 'desktop' | 'server';
    platform?: string;
    hue?: number;
    online?: boolean;
    battery?: [number, boolean] | null;
    /** Length of the longer side in px. */
    size?: number;
    /** 'long': size is the longer side. 'height': every device is exactly this tall. */
    fit?: 'long' | 'height';
  } = $props();

  const uid = `nas${counter++}`;
  const apple = $derived(platform === 'macos' || platform === 'ios');
  const model = $derived.by<Model | null>(() => {
    if (kind === 'phone') return apple ? MODELS.iphone : MODELS.pixel;
    if (kind === 'tablet') return MODELS.tablet;
    if (kind === 'laptop') return platform === 'macos' ? MODELS.macbook : MODELS.surfacebook;
    if (kind === 'desktop') return platform === 'macos' ? MODELS.imac : MODELS.studio;
    return null;
  });
  const scale = $derived(model ? size / (fit === 'height' ? model.h : Math.max(model.w, model.h)) : 1);
  const pct = $derived(battery ? Math.max(3, Math.min(100, battery[0])) : 0);
  const charging = $derived(!!battery && battery[1]);
  const mobile = $derived(kind === 'phone' || kind === 'tablet');
</script>

{#if model}
  <div class="wrap" class:off={!online} style="--h:{hue}; width:{model.w * scale}px; height:{model.h * scale}px" aria-hidden="true">
    <div class="device {model.cls}" style="transform: scale({scale}); transform-origin: 0 0">
      <div class="device-frame">
        <div class="device-screen">
          <div class="ui" class:dark={!online}>
            {#if online}
              <div class="glow"></div>
              {#if mobile}
                <div class="clock">9:41</div>
                <div class="date">Tuesday 10 November</div>
                {#if battery}
                  <div class="pill" class:charging>
                    {#if charging}<span class="halo"></span>{/if}
                    <span class="lvl" style="width:{pct}%"></span>
                    {#if charging}<svg class="bolt" viewBox="0 0 24 24"><path d="M13.5 2 5 13.5h6L9.5 22 19 9.5h-6.2z" /></svg>{/if}
                  </div>
                {:else}
                  <div class="apps">{#each [0, 1, 2, 3, 4, 5, 6, 7] as i}<i style="opacity:{0.55 - i * 0.03}"></i>{/each}</div>
                {/if}
                <div class="home"></div>
              {:else}
                <div class="win w1"><b></b><i></i><i class="s"></i></div>
                <div class="win w2"><b></b></div>
                <div class="dock"></div>
              {/if}
            {/if}
          </div>
        </div>
      </div>
      <div class="device-stripe"></div>
      <div class="device-header"></div>
      <div class="device-sensors"></div>
      <div class="device-btns"></div>
      <div class="device-power"></div>
    </div>
  </div>
{:else}
  <!-- A compact home server with four drive bays (ZimaOS box). -->
  <svg class="nas" class:off={!online} style="width:{size}px; height:{size}px" viewBox="0 0 120 120" fill="none" aria-hidden="true">
    <defs>
      <linearGradient id="{uid}-b" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#43444b" />
        <stop offset="0.5" stop-color="#1e1f23" />
        <stop offset="1" stop-color="#2c2d33" />
      </linearGradient>
      <linearGradient id="{uid}-g" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#fff" stop-opacity="0.2" />
        <stop offset="0.45" stop-color="#fff" stop-opacity="0" />
      </linearGradient>
    </defs>
    <ellipse cx="60" cy="115" rx="34" ry="3" fill="#000" opacity="0.14" />
    <rect x="24" y="10" width="72" height="98" rx="9" fill="url(#{uid}-b)" />
    <rect x="24.4" y="10.4" width="71.2" height="97.2" rx="8.6" stroke="#fff" stroke-opacity="0.2" stroke-width="0.8" />
    {#each [0, 1, 2, 3] as i}
      <rect x="31" y={18 + i * 18} width="58" height="13" rx="3.2" fill="#0b0b0d" />
      <rect x="33" y={21.5 + i * 18} width="34" height="1.8" rx="0.9" fill="#fff" opacity="0.14" />
      <rect x="33" y={25 + i * 18} width="22" height="1.4" rx="0.7" fill="#fff" opacity="0.09" />
      <circle class="led" style="animation-delay: {i * 0.45}s" cx="81" cy={24.5 + i * 18} r="2" />
    {/each}
    <circle cx="60" cy="97" r="4.2" fill="#0b0b0d" />
    <circle class="pwr" cx="60" cy="97" r="2.1" />
    <path d="M24 10 H96 L24 72 Z" fill="url(#{uid}-g)" />
  </svg>
{/if}

<style>
  .wrap {
    position: relative;
    flex: none;
    filter: drop-shadow(0 6px 10px rgba(0, 0, 0, 0.18));
  }
  .wrap.off {
    filter: drop-shadow(0 6px 10px rgba(0, 0, 0, 0.14)) saturate(0.85);
  }
  /* The screen: a wallpaper in the owner's colour. */
  .ui {
    position: absolute;
    inset: 0;
    container-type: size;
    overflow: hidden;
    background: linear-gradient(150deg, hsl(var(--h) 85% 70%) 0%, hsl(calc(var(--h) + 30) 75% 58%) 55%, hsl(calc(var(--h) + 64) 70% 42%) 100%);
    color: #fff;
    font-family: 'Geist', system-ui, sans-serif;
  }
  .ui.dark {
    background: linear-gradient(150deg, #26272c, #141417);
  }
  .glow {
    position: absolute;
    width: 90cqmin;
    height: 90cqmin;
    right: -25cqmin;
    top: -30cqmin;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.13);
  }
  .clock {
    position: absolute;
    top: 17cqh;
    width: 100%;
    text-align: center;
    font-weight: 600;
    font-size: 21cqw;
    letter-spacing: -0.02em;
  }
  .date {
    position: absolute;
    top: 13cqh;
    width: 100%;
    text-align: center;
    font-size: 5cqw;
    opacity: 0.9;
  }
  .pill {
    position: absolute;
    left: 50%;
    bottom: 22cqh;
    width: 46cqw;
    height: 9cqw;
    transform: translateX(-50%);
    border-radius: 99px;
    background: rgba(0, 0, 0, 0.3);
    padding: 1.1cqw;
  }
  .lvl {
    display: block;
    height: 100%;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.94);
    transition: width 0.6s ease;
  }
  .charging .lvl {
    animation: shimmer 1.8s ease-in-out infinite;
  }
  .halo {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 150%;
    height: 400%;
    transform: translate(-50%, -50%);
    border-radius: 50%;
    background: radial-gradient(closest-side, rgba(61, 220, 132, 0.6), rgba(61, 220, 132, 0));
    animation: halo 2.2s ease-in-out infinite;
    pointer-events: none;
  }
  .bolt {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 7cqw;
    height: 7cqw;
    transform: translate(-50%, -50%);
    fill: #12b358;
    stroke: #fff;
    stroke-width: 1.4;
    stroke-linejoin: round;
    animation: pulse 1.6s ease-in-out infinite;
  }
  .apps {
    position: absolute;
    left: 14cqw;
    right: 14cqw;
    bottom: 18cqh;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4cqw;
  }
  .apps i {
    aspect-ratio: 1;
    border-radius: 24%;
    background: #fff;
  }
  .home {
    position: absolute;
    bottom: 2.4cqh;
    left: 50%;
    width: 32cqw;
    height: 1.2cqh;
    min-height: 3px;
    transform: translateX(-50%);
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.85);
  }
  /* Desktop screens: a couple of windows and a dock. */
  .win {
    position: absolute;
    border-radius: 2.2cqw;
    background: rgba(255, 255, 255, 0.9);
    box-shadow: 0 1.2cqw 3cqw rgba(0, 0, 0, 0.18);
  }
  .win b {
    display: block;
    height: 6cqh;
    border-radius: 2.2cqw 2.2cqw 0 0;
    background: rgba(0, 0, 0, 0.08);
  }
  .win i {
    display: block;
    height: 2.6cqh;
    margin: 4cqh 4cqw 0;
    border-radius: 99px;
    background: rgba(0, 0, 0, 0.14);
  }
  .win i.s {
    width: 40%;
  }
  .w1 {
    left: 9cqw;
    top: 16cqh;
    width: 46cqw;
    height: 46cqh;
  }
  .w2 {
    right: 8cqw;
    top: 30cqh;
    width: 34cqw;
    height: 38cqh;
    background: rgba(255, 255, 255, 0.62);
  }
  .dock {
    position: absolute;
    left: 50%;
    bottom: 3cqh;
    width: 34cqw;
    height: 6cqh;
    transform: translateX(-50%);
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.4);
  }
  .nas {
    flex: none;
    overflow: visible;
  }
  .led {
    fill: #3ddc84;
    animation: blink 1.9s ease-in-out infinite;
  }
  .pwr {
    fill: #3ddc84;
  }
  .off .led,
  .off .pwr {
    fill: #55565c;
    animation: none;
  }
  @keyframes pulse {
    0%,
    100% {
      transform: translate(-50%, -50%) scale(1);
    }
    50% {
      transform: translate(-50%, -50%) scale(1.2);
    }
  }
  @keyframes halo {
    0%,
    100% {
      opacity: 0.55;
    }
    50% {
      opacity: 1;
    }
  }
  @keyframes shimmer {
    50% {
      opacity: 0.7;
    }
  }
  @keyframes blink {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.25;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bolt,
    .halo,
    .led,
    .charging .lvl {
      animation: none;
    }
  }
</style>
