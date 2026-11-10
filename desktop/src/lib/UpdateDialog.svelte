<script lang="ts">
  import { onMount } from 'svelte';
  import Dialog from '$lib/Dialog.svelte';
  import { api, errText, type UpdateInfo } from '$lib/api';

  let { info, onclose, say }: { info: UpdateInfo; onclose: () => void; say: (t: string, err?: boolean) => void } = $props();

  let working = $state(false);
  let progress = $state<[number, number] | null>(null);

  // Release notes: headings, bullets and plain lines. Markdown noise is dropped.
  type Line = { kind: 'h' | 'li' | 'p'; text: string };
  const clean = (t: string) => t.replace(/\*\*/g, '').replace(/`/g, '');
  const lines = $derived.by<Line[]>(() =>
    info.notes
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter((l) => l && !/^\**full changelog/i.test(l))
      .map((l): Line => {
        const li = l.match(/^[-*]\s+(.*)/);
        if (li) return { kind: 'li', text: clean(li[1]) };
        if (/^#+\s/.test(l)) return { kind: 'h', text: clean(l.replace(/^#+\s*/, '')) };
        return { kind: 'p', text: clean(l) };
      })
  );

  onMount(() => {
    let un: (() => void) | null = null;
    import('@tauri-apps/api/event')
      .then(({ listen }) => listen<[number, number]>('update-progress', (e) => (progress = e.payload)))
      .then((u) => (un = u))
      .catch(() => {});
    return () => un?.();
  });

  const mb = (n: number) => `${(n / 1e6).toFixed(1)} MB`;

  async function go() {
    working = true;
    try {
      await api.installUpdate(info);
    } catch (e) {
      working = false;
      progress = null;
      say(errText(e), true);
    }
  }
</script>

<Dialog title="Zima Connect {info.latest}" {onclose}>
  <p class="lead">You have {info.current}. Your devices, people and settings stay as they are.</p>
  <div class="notes" role="region" tabindex="0" aria-label="What is new">
    {#each lines as l}
      {#if l.kind === 'h'}<h4>{l.text}</h4>{:else if l.kind === 'li'}<div class="li">{l.text}</div>{:else}<p>{l.text}</p>{/if}
    {:else}
      <p>No release notes.</p>
    {/each}
  </div>
  {#if working}
    <div class="bar" role="progressbar" aria-valuenow={progress ? Math.round((progress[0] / Math.max(progress[1], 1)) * 100) : 0} aria-valuemin="0" aria-valuemax="100">
      <i style="width: {progress ? Math.round((progress[0] / Math.max(progress[1], 1)) * 100) : 4}%"></i>
    </div>
    <div class="sub num">{progress ? `${mb(progress[0])} of ${mb(progress[1])}` : 'Starting…'} · Zima Connect restarts by itself.</div>
  {/if}
  <div class="actions">
    <button class="btn btn-secondary" onclick={onclose} disabled={working}>Later</button>
    {#if info.can_install}
      <button class="btn btn-primary" onclick={go} disabled={working}>{working ? 'Updating…' : 'Update and restart'}</button>
    {:else}
      <button class="btn btn-primary" onclick={() => api.openExternal(info.page)}>Open download page</button>
    {/if}
  </div>
</Dialog>

<style>
  .notes {
    max-height: 260px;
    overflow: auto;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--raised);
  }
  .notes p,
  .notes h4,
  .li {
    margin: 0 0 6px;
  }
  .notes h4 {
    font-size: 13px;
  }
  .li {
    padding-left: 14px;
    position: relative;
  }
  .li::before {
    content: '–';
    position: absolute;
    left: 0;
    color: var(--muted);
  }
  .bar {
    height: 6px;
    border-radius: 99px;
    background: var(--active);
    overflow: hidden;
    margin-top: 14px;
  }
  .bar i {
    display: block;
    height: 100%;
    background: var(--primary);
    transition: width 0.2s;
  }
</style>
