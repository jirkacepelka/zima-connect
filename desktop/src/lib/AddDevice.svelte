<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import Dialog from '$lib/Dialog.svelte';
  import Icon from '$lib/Icon.svelte';
  import Qr from '$lib/Qr.svelte';
  import { api, errText } from '$lib/api';

  let {
    start,
    pairedCount,
    say,
    onclose,
    ondone
  }: {
    start: 'server' | 'show' | 'enter';
    pairedCount: number;
    say: (t: string, err?: boolean) => void;
    onclose: () => void;
    ondone: () => void;
  } = $props();

  let tab = $state<'server' | 'show' | 'enter'>('server');
  let serverCode = $state('');
  let yourName = $state('');
  let enteredCode = $state('');
  let myCode = $state('');
  let loading = $state(false);
  let copied = $state(false);
  let working = $state(false);
  const startCount = untrack(() => pairedCount);

  async function setTab(t: 'server' | 'show' | 'enter') {
    tab = t;
    if (t === 'show' && !myCode) {
      loading = true;
      try {
        myCode = await api.pairingCode();
      } catch (e) {
        say(errText(e), true);
      } finally {
        loading = false;
      }
    }
  }
  onMount(() => {
    setTab(start);
  });

  // The other device paired with our code: nothing more to do here.
  $effect(() => {
    if (tab === 'show' && pairedCount > startCount) {
      say('Device paired');
      ondone();
    }
  });

  async function join() {
    if (!serverCode.trim() || working) return;
    working = true;
    try {
      const name = await api.join(serverCode.trim(), yourName.trim() || null);
      say(`Joined ${name}`);
      ondone();
    } catch (e) {
      say(errText(e), true);
    } finally {
      working = false;
    }
  }
  async function pair() {
    if (!enteredCode.trim() || working) return;
    working = true;
    try {
      const name = await api.pair(enteredCode.trim());
      say(`Paired with ${name}`);
      ondone();
    } catch (e) {
      say(errText(e), true);
    } finally {
      working = false;
    }
  }
  async function copy() {
    try {
      await navigator.clipboard.writeText(myCode);
      copied = true;
      setTimeout(() => (copied = false), 1600);
    } catch {
      say('Could not copy. Select the code and copy it manually.', true);
    }
  }
</script>

<Dialog title="Add device" {onclose}>
  <div class="seg" role="tablist">
    <button role="tab" aria-selected={tab === 'server'} class:on={tab === 'server'} onclick={() => setTab('server')}>Join server</button>
    <button role="tab" aria-selected={tab === 'show'} class:on={tab === 'show'} onclick={() => setTab('show')}>Show code</button>
    <button role="tab" aria-selected={tab === 'enter'} class:on={tab === 'enter'} onclick={() => setTab('enter')}>Enter code</button>
  </div>

  {#if tab === 'server'}
    <p class="lead">Open your server’s page (or ask the household owner) for an invitation and paste its code. After this your devices find each other automatically.</p>
    <div class="stack">
      <label class="label" for="sc" style="margin:0">Server code</label>
      <textarea id="sc" class="field" rows="4" placeholder="zch1-…" bind:value={serverCode} spellcheck="false"></textarea>
      <label class="label" for="yn" style="margin:0">Your name <span class="sub">(only if you are new)</span></label>
      <input id="yn" class="field" bind:value={yourName} maxlength="40" autocomplete="off" />
    </div>
    <div class="actions">
      <button class="btn btn-secondary" onclick={onclose}>Cancel</button>
      <button class="btn btn-primary" disabled={!serverCode.trim() || working} onclick={join}>{working ? 'Connecting…' : 'Join'}</button>
    </div>
  {:else if tab === 'show'}
    <p class="lead">Pair two devices directly, without a server. On the other device choose <b>Enter code</b>. It works once and expires in 15 minutes.</p>
    <div class="codebox">{loading ? 'Creating a code…' : myCode}</div>
    {#if myCode}<div class="qr"><Qr text={myCode} /></div>{/if}
    <div class="actions">
      <button class="btn btn-secondary" onclick={onclose}>Close</button>
      <button class="btn btn-primary" disabled={!myCode} onclick={copy}><Icon name={copied ? 'check' : 'copy'} size={15} /> {copied ? 'Copied' : 'Copy code'}</button>
    </div>
  {:else}
    <p class="lead">Direct pairing without a server: paste the code shown on the other device.</p>
    <textarea class="field" rows="4" placeholder="zc1-…" bind:value={enteredCode} spellcheck="false" aria-label="Pairing code"></textarea>
    <div class="actions">
      <button class="btn btn-secondary" onclick={onclose}>Cancel</button>
      <button class="btn btn-primary" disabled={!enteredCode.trim() || working} onclick={pair}>{working ? 'Connecting…' : 'Pair'}</button>
    </div>
  {/if}
</Dialog>
