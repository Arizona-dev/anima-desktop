// Injected into the Anima web UI (server pages only) by the desktop app.
(() => {
  if (location.hostname !== '127.0.0.1' && location.hostname !== 'localhost') return;

  // --- Style fixes -------------------------------------------------------------------------
  // WebKit's native <select> ignores padding-left: the namespace icon overlapped the text.
  // "About Anima" (last item of the sidebar nav) is pinned to the bottom of the sidebar.
  const css = `
select.field {
  -webkit-appearance: none; appearance: none;
  padding-right: 1.75rem;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%23999' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right .6rem center;
}
nav[aria-label="Main"] { display: flex; flex-direction: column; }
nav[aria-label="Main"] > * { flex-shrink: 0; }
nav[aria-label="Main"] > button:last-child { margin-top: auto !important; }
#desktop .path { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12px; word-break: break-all; }
`;

  // --- "Desktop app" section in Settings ---------------------------------------------------
  // Same markup and classes as the page's own sections, so it follows the Anima theme.
  const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

  const sectionHtml = `
<div class="flex items-start justify-between gap-3 px-5 pt-5">
  <div class="min-w-0">
    <h2 id="desktop-title" class="text-sm font-semibold text-ink">Desktop app</h2>
    <p class="text-[13px] text-ink-muted mt-0.5">How this computer starts and runs the Anima server.</p>
  </div>
</div>
<div class="p-5 space-y-4">
  <label class="flex items-center justify-between gap-4 cursor-pointer">
    <span class="min-w-0">
      <span class="block text-sm font-medium text-ink">Launch at login</span>
      <span class="block text-[13px] text-ink-muted" data-k="loginHint"></span>
    </span>
    <span class="relative shrink-0">
      <input type="checkbox" role="switch" data-k="login" class="sr-only peer">
      <span class="block w-9 h-5 rounded-full transition-colors bg-warm-border-strong peer-checked:bg-accent"></span>
      <span class="absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-card shadow-sm transition-transform peer-checked:translate-x-4"></span>
    </span>
  </label>
  <div class="flex flex-wrap items-center justify-between gap-3">
    <div class="min-w-0 flex-1">
      <p class="text-sm font-medium text-ink">Server</p>
      <p class="text-[13px] text-ink-muted" data-k="server"></p>
    </div>
    <button class="btn btn-secondary shrink-0" data-k="switch"></button>
  </div>
  <div class="flex flex-wrap items-center justify-between gap-3">
    <div class="min-w-0 flex-1">
      <p class="text-sm font-medium text-ink">Data folder</p>
      <p class="text-[13px] text-ink-muted path" data-k="data"></p>
      <p class="text-xs text-ink-muted mt-1" data-k="dataHint"></p>
    </div>
    <button class="btn btn-secondary shrink-0" data-k="openData">Open</button>
  </div>
  <div class="flex flex-wrap items-center justify-between gap-3">
    <div class="min-w-0 flex-1">
      <p class="text-sm font-medium text-ink">API key file <span class="text-ink-muted font-normal">(optional)</span></p>
      <p class="text-[13px] text-ink-muted path" data-k="key"></p>
      <p class="text-xs text-ink-muted mt-1">Passed to the server as its embedding, LLM and reranker key.</p>
    </div>
    <div class="flex gap-2 shrink-0">
      <button class="btn btn-secondary" data-k="clearKey">Default</button>
      <button class="btn btn-secondary" data-k="pickKey">Change…</button>
    </div>
  </div>
  <p class="text-xs" data-k="msg"></p>
</div>`;

  function render(sec, s) {
    const $ = (k) => sec.querySelector(`[data-k="${k}"]`);
    $('login').checked = s.login.enabled || s.login.needs_approval;
    $('loginHint').textContent = s.login.needs_approval
      ? 'Waiting for your approval in System Settings → General → Login Items.'
      : 'Anima starts in the menu bar or tray, without a window, and starts the memory server.';
    const v = s.version ? `version ${s.version}` : 'not found';
    $('server').textContent = s.bundled
      ? `Bundled with the app, ${v}.`
      : `External installation in ${s.externalDir}, ${v}.`;
    $('switch').textContent = s.bundled ? 'Use an external installation…' : 'Use the bundled server';
    $('data').textContent = s.dataDir;
    $('dataHint').textContent = [
      `anima.db, config.toml, logs and models. Port ${s.port}, from config.toml.`,
      s.bundled && s.migration ? s.migration + '.' : '',
    ].filter(Boolean).join(' ');
    $('key').textContent = s.keyFile + (s.keyDefault ? ' (default)' : '');
  }

  function wire(sec) {
    const $ = (k) => sec.querySelector(`[data-k="${k}"]`);
    let bundled = true;
    const msg = (text, error) => { $('msg').textContent = text; $('msg').className = 'text-xs ' + (error ? 'text-danger' : 'text-ink-muted'); };
    const run = async (p, ok) => {
      try { const s = await p; if (s) { bundled = s.bundled; render(sec, s); } if (ok) msg(ok); } catch (e) { msg(String(e), true); }
    };
    sec._setBundled = (b) => { bundled = b; };
    $('login').addEventListener('change', (e) =>
      run(invoke('set_launch_at_login', { on: e.target.checked }).then(() => invoke('get_settings'))));
    $('switch').addEventListener('click', () =>
      run(invoke(bundled ? 'use_external_server' : 'use_bundled_server'), 'Saved. The server restarts.'));
    $('openData').addEventListener('click', () => invoke('open_data_dir'));
    $('pickKey').addEventListener('click', () => run(invoke('choose_key_file'), 'Saved. Used the next time the server starts.'));
    $('clearKey').addEventListener('click', () => run(invoke('clear_key_file'), 'Saved.'));
  }

  function inject() {
    if (location.pathname !== '/settings') return;
    const nav = document.querySelector('nav[aria-label="Settings sections"] ul');
    // Before "Backup and restore": the sections above it come and go with the server's
    // capabilities, and React inserts their links at their own place.
    const anchor = document.getElementById('backup');
    if (!nav || !anchor || document.getElementById('desktop')) return;

    const sec = document.createElement('section');
    sec.id = 'desktop';
    sec.setAttribute('aria-labelledby', 'desktop-title');
    sec.className = 'card scroll-mt-4 overflow-hidden';
    sec.innerHTML = sectionHtml;
    anchor.parentNode.insertBefore(sec, anchor);

    const link = nav.querySelector('a[href="#backup"]');
    if (link && !nav.querySelector('a[href="#desktop"]')) {
      const li = link.parentElement.cloneNode(true);
      li.querySelector('a').setAttribute('href', '#desktop');
      li.querySelector('a').textContent = 'Desktop app';
      nav.insertBefore(li, link.parentElement);
    }
    wire(sec);
    invoke('get_settings').then((s) => { sec._setBundled(s.bundled); render(sec, s); }).catch((e) => {
      sec.querySelector('[data-k="msg"]').textContent = 'Desktop settings unavailable: ' + String(e);
    });
    // Sections above load their data asynchronously and push this one down: scroll again.
    if (location.hash === '#desktop') {
      for (const ms of [0, 300, 800, 1500]) setTimeout(() => sec.scrollIntoView({ block: 'start' }), ms);
    }
  }

  const start = () => {
    const st = document.createElement('style'); st.textContent = css; document.head.appendChild(st);
    // The UI is a single-page app: watch for the Settings page being (re)rendered.
    new MutationObserver(inject).observe(document.body, { childList: true, subtree: true });
    inject();
  };
  if (document.body) start(); else document.addEventListener('DOMContentLoaded', start);
})();
