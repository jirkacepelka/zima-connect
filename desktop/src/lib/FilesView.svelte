<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import { api, errText } from '$lib/api';
  import { size } from '$lib/devices';

  let { peerId, name, say }: { peerId: string; name: string; say: (t: string, err?: boolean) => void } = $props();

  type Entry = { name: string; dir: boolean; size: number; mtime: number };
  let path = $state('/');
  let entries = $state<Entry[]>([]);
  let loading = $state(true);
  let problem = $state('');
  let saving = $state<string | null>(null);

  const crumbs = $derived(path.split('/').filter(Boolean));
  const sorted = $derived([...entries].sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' })));

  async function open(p: string) {
    loading = true;
    problem = '';
    try {
      entries = (await api.filesList(peerId, p)).entries;
      path = p;
    } catch (e) {
      problem = errText(e);
    } finally {
      loading = false;
    }
  }
  onMount(() => {
    open('/');
  });

  const join = (n: string) => (path === '/' ? '/' : path + '/') + n;
  const upTo = (i: number) => '/' + crumbs.slice(0, i + 1).join('/');

  async function copy(e: Entry) {
    saving = e.name;
    try {
      const where = await api.filesSave(peerId, join(e.name));
      say(`Copied ${e.name} to your Downloads`);
      api.reveal(where);
    } catch (err) {
      say(errText(err), true);
    } finally {
      saving = null;
    }
  }
  const when = (s: number) => (s ? new Date(s * 1000).toLocaleDateString() : '');
</script>

<div class="files">
  <nav class="crumbs" aria-label="Location">
    <button class="link" onclick={() => open('/')}>{name}</button>
    {#each crumbs as c, i}
      <span class="sep">/</span>
      <button class="link" onclick={() => open(upTo(i))}>{c}</button>
    {/each}
  </nav>

  {#if problem}
    <div class="note"><Icon name="alert" size={18} /> {problem}</div>
  {:else if loading}
    <div class="note">Loading…</div>
  {:else if sorted.length === 0}
    <div class="note">This folder is empty.</div>
  {:else}
    <ul class="list">
      {#each sorted as e (e.name)}
        <li>
          {#if e.dir}
            <button class="row dir" onclick={() => open(join(e.name))}>
              <Icon name="shelf" size={16} /><span class="n">{e.name}</span><span class="sub num">{when(e.mtime)}</span>
            </button>
          {:else}
            <div class="row">
              <Icon name="file" size={16} /><span class="n" title={e.name}>{e.name}</span>
              <span class="sub num">{size(e.size)}</span><span class="sub num when">{when(e.mtime)}</span>
              <button class="btn btn-secondary tiny" disabled={saving !== null} onclick={() => copy(e)}>{saving === e.name ? 'Copying…' : 'Copy here'}</button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .files {
    display: grid;
    gap: 12px;
  }
  .crumbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .sep {
    color: var(--muted);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 52vh;
    overflow: auto;
    border-top: 1px solid var(--rule);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 8px 6px;
    text-align: left;
    border-bottom: 1px solid var(--rule);
  }
  .row.dir:hover {
    background: var(--raised);
  }
  .n {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .when {
    min-width: 76px;
    text-align: right;
  }
  .tiny {
    height: 28px;
    padding: 0 10px;
  }
  .note {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 28px 6px;
    color: var(--muted);
  }
</style>
