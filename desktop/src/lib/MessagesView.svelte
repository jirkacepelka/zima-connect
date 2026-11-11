<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Icon from '$lib/Icon.svelte';
  import { api, errText, type Contact, type SmsMessage, type SmsThread } from '$lib/api';
  import { hueFor } from '$lib/devices';

  let { c, say, onBack }: { c: Contact; say: (t: string, err?: boolean) => void; onBack: () => void } = $props();

  let threads = $state<SmsThread[]>([]);
  let messages = $state<SmsMessage[]>([]);
  let open = $state<SmsThread | null>(null);
  let loading = $state(true);
  let problem = $state('');
  let draft = $state('');
  let newTo = $state('');
  let composing = $state(false);
  let scroller: HTMLDivElement | undefined = $state();

  const hue = $derived(hueFor(c));

  async function loadThreads() {
    try {
      threads = await api.smsThreads(c.id);
      problem = '';
    } catch (e) {
      problem = errText(e);
    } finally {
      loading = false;
    }
  }
  async function loadThread() {
    if (!open) return;
    try {
      messages = await api.smsThread(c.id, open.id);
      await tick();
      scroller?.scrollTo(0, scroller.scrollHeight);
    } catch (e) {
      say(errText(e), true);
    }
  }
  onMount(() => {
    loadThreads();
    let un: (() => void) | null = null;
    import('@tauri-apps/api/event')
      .then(({ listen }) =>
        listen('sms-received', () => {
          loadThreads();
          loadThread();
        })
      )
      .then((u) => (un = u))
      .catch(() => {});
    const t = setInterval(() => {
      loadThreads();
      loadThread();
    }, 15000);
    return () => {
      un?.();
      clearInterval(t);
    };
  });

  async function pick(t: SmsThread) {
    open = t;
    composing = false;
    messages = [];
    await loadThread();
  }

  async function send() {
    const to = composing ? newTo.trim() : open?.address;
    const body = draft.trim();
    if (!to || !body) return;
    try {
      await api.smsSend(c.id, to, body);
      draft = '';
      say('Sent through ' + c.name);
      if (composing) {
        composing = false;
        newTo = '';
        await loadThreads();
        open = threads.find((t) => t.address.replace(/\D/g, '').endsWith(to.replace(/\D/g, '').slice(-9))) ?? null;
      }
      setTimeout(loadThread, 1200);
    } catch (e) {
      say(errText(e), true);
    }
  }

  const label = (t: SmsThread) => t.name ?? t.address;
  function when(ts: number) {
    const d = new Date(ts);
    const today = new Date();
    return d.toDateString() === today.toDateString() ? d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : d.toLocaleDateString();
  }
</script>

<div class="title-row">
  <div>
    <button class="link back" onclick={onBack}>← {c.name}</button>
    <h1>Text messages</h1>
    <div class="finding">Read and answer the SMS of {c.name} from here. Messages stay on the phone.</div>
  </div>
  <div class="actions"><button class="btn btn-secondary" onclick={() => { composing = true; open = null; }}><Icon name="plus" size={15} /> New message</button></div>
</div>

{#if problem}
  <section class="card"><div class="empty-line" style="border-top:0">{problem}</div></section>
{:else}
  <div class="split" style="--h:{hue}">
    <div class="list" aria-label="Conversations">
      {#each threads as t (t.id)}
        <button class="thread" class:on={open?.id === t.id} class:unread={t.unread} onclick={() => pick(t)}>
          <span class="av">{label(t).slice(0, 1).toUpperCase()}</span>
          <span class="body"><b>{label(t)}</b><span class="sub">{t.snippet}</span></span>
          <span class="sub num">{when(t.date)}</span>
        </button>
      {:else}
        <div class="empty-line" style="border-top:0">{loading ? 'Loading…' : 'No conversations.'}</div>
      {/each}
    </div>

    <div class="chat">
      {#if composing}
        <label class="label" for="to">To</label>
        <input id="to" class="field" bind:value={newTo} placeholder="Phone number" inputmode="tel" autocomplete="off" />
        <div class="grow"></div>
      {:else if open}
        <div class="head"><span class="av">{label(open).slice(0, 1).toUpperCase()}</span><b>{label(open)}</b><span class="sub">{open.address}</span></div>
        <div class="msgs" bind:this={scroller}>
          {#each messages as m (m.id)}
            <div class="bubble" class:me={m.outgoing}>{m.body}<span class="t num">{when(m.date)}</span></div>
          {/each}
        </div>
      {:else}
        <div class="none">Choose a conversation.</div>
      {/if}
      {#if composing || open}
        <form class="composer" onsubmit={(e) => { e.preventDefault(); send(); }}>
          <input class="field" bind:value={draft} placeholder="Message" aria-label="Message" autocomplete="off" />
          <button class="btn btn-primary" disabled={!draft.trim() || (composing && !newTo.trim())}>Send</button>
        </form>
      {/if}
    </div>
  </div>
{/if}

<style>
  .back {
    font-size: 13px;
    margin-bottom: 6px;
    display: inline-block;
  }
  .split {
    display: grid;
    grid-template-columns: 280px minmax(0, 1fr);
    gap: 24px;
    height: min(560px, calc(100vh - 230px));
  }
  .list {
    overflow-y: auto;
    border-right: 1px solid var(--rule);
    padding-right: 8px;
  }
  .thread {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 9px 8px;
    border-radius: 8px;
    text-align: left;
  }
  .thread:hover {
    background: var(--raised);
  }
  .thread.on {
    background: var(--active);
  }
  .thread .body {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .thread b,
  .thread .sub {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .thread.unread b {
    font-weight: 700;
  }
  .av {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    flex: none;
    font-weight: 600;
    background: hsl(var(--h) 60% 90%);
    color: hsl(var(--h) 55% 28%);
  }
  @media (prefers-color-scheme: dark) {
    .av {
      background: hsl(var(--h) 35% 24%);
      color: hsl(var(--h) 70% 85%);
    }
  }
  .chat {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-bottom: 12px;
  }
  .msgs {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 4px 4px 12px;
  }
  .bubble {
    align-self: flex-start;
    max-width: 72%;
    padding: 8px 12px;
    border-radius: 16px 16px 16px 4px;
    background: var(--active);
    user-select: text;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .bubble.me {
    align-self: flex-end;
    border-radius: 16px 16px 4px 16px;
    background: hsl(var(--h) 70% 60%);
    color: #fff;
  }
  .t {
    display: block;
    font-size: 11px;
    opacity: 0.7;
    margin-top: 2px;
  }
  .none {
    margin: auto;
    color: var(--muted);
  }
  .grow {
    flex: 1;
  }
  .composer {
    display: flex;
    gap: 8px;
    padding-top: 10px;
  }
</style>
