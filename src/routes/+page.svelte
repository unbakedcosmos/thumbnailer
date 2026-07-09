<script>
  import { onMount } from 'svelte';
  import { app, init, generateSelected, startBatch, pauseBatch } from '$lib/state.svelte.js';
  import Header from '$lib/Header.svelte';
  import QueueRail from '$lib/QueueRail.svelte';
  import Editor from '$lib/Editor.svelte';
  import EmptyState from '$lib/EmptyState.svelte';
  import SettingsModal from '$lib/SettingsModal.svelte';
  import TemplateGallery from '$lib/TemplateGallery.svelte';
  import TemplateEditor from '$lib/TemplateEditor.svelte';
  import FfmpegPrompt from '$lib/FfmpegPrompt.svelte';

  onMount(() => {
    init();
  });

  // Keyboard-first (EXPERIENCE Interaction Primitives): ↑/↓ select,
  // Enter = generate selected, Space = pause/resume batch, F = follow toggle.
  function onKey(e) {
    const tag = e.target?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA') return;
    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault();
      if (!app.jobs.length) return;
      const i = app.jobs.findIndex((j) => j.id === app.selectedId);
      const next =
        e.key === 'ArrowDown' ? Math.min(app.jobs.length - 1, i + 1) : Math.max(0, i - 1);
      app.selectedId = app.jobs[next < 0 ? 0 : next].id;
    } else if (e.key === 'Enter') {
      if (e.target?.tagName === 'BUTTON') return;
      generateSelected();
    } else if (e.key === ' ') {
      if (e.target?.tagName === 'BUTTON') return;
      e.preventDefault();
      if (app.batch.status === 'running') pauseBatch();
      else if (app.batch.status === 'paused' || app.batch.status === 'ready') startBatch();
    } else if (e.key === 'f' || e.key === 'F') {
      app.follow = !app.follow;
    } else if (e.key === 'Escape') {
      if (app.templateEditor) app.templateEditor = null;
      else if (app.templateGalleryOpen) app.templateGalleryOpen = false;
      else if (app.settingsOpen) app.settingsOpen = false;
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<svelte:head>
  <title>thumbnailer</title>
</svelte:head>

<div class="app">
  <Header />
  <div class="body">
    {#if app.jobs.length}
      <QueueRail />
      <Editor />
    {:else}
      <EmptyState />
    {/if}
  </div>
  {#if app.settingsOpen}
    <SettingsModal />
  {/if}
  {#if app.templateGalleryOpen}
    <TemplateGallery />
  {/if}
  <TemplateEditor />
  {#if app.ffmpegReady === false && !app.ffmpegPromptDismissed}
    <FfmpegPrompt />
  {/if}
</div>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    position: relative;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
    position: relative;
  }
</style>
