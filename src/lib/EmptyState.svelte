<script>
  import { pickFolder } from '$lib/state.svelte.js';
</script>

<div class="wrap">
  <div
    class="drop"
    onclick={pickFolder}
    onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && pickFolder()}
    role="button"
    tabindex="0"
  >
    <div class="t">Drop a folder to start</div>
    <div class="s">Every video inside becomes a row in the queue</div>
    <button class="btn-primary">+ Add folder</button>
  </div>
</div>

<style>
  .wrap {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px;
  }
  .drop {
    border: 1px dashed var(--border);
    border-radius: var(--r-md);
    padding: 60px 90px;
    text-align: center;
    cursor: pointer;
  }
  .drop:hover {
    border-color: var(--text-dim);
  }
  .t {
    font-size: 15px;
    font-weight: 700;
    margin-bottom: 8px;
  }
  .s {
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 20px;
  }
</style>
