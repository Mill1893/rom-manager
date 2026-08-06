/* THROWAWAY PROTOTYPE — Variant A: "Library Browser".
   Three-pane media manager: sidebar nav, box-art grid, inspector.
   Sync happens in a modal wizard launched from a pack or a target. */

const VA = {
  view: { kind: 'platform', key: 'all' },
  selectedGame: null,
  wizard: null, // {step, packId, targetId, plan, planSt, scenario}
  lens: 't-sd',   // device-presence lens: which target tiles show status for
  filter: 'all',  // all | on | off  (relative to the lens target)
  vmode: 'grid',  // grid | list — list is the fast scan across systems
};

function renderVariantA(root) {
  const platCounts = PLATFORMS.map(p => ({ p, n: GAMES.filter(g => g.platform === p.key).length }));
  const navItem = (icon, label, on, badge) =>
    `<button class="nav-i ${on ? 'on' : ''}" data-nav><span class="lbl"><span class="nav-ic">${icon}</span>${esc(label)}</span><span class="cnt">${badge || ''}</span></button>`;

  root.innerHTML = `
  <div class="va">
    <aside class="va-side">
      <div class="va-brand"><div class="logo"></div><h1>ROM Manager</h1></div>
      <div class="va-nav">
        <div class="nav-h">Library</div>
        ${navItem('▦', 'All Games', VA.view.kind === 'platform' && VA.view.key === 'all', GAMES.length)}
        ${platCounts.map(x => navItem('▸', x.p.short + ' — ' + x.p.name, VA.view.kind === 'platform' && VA.view.key === x.p.key, x.n)).join('')}
        ${navItem('⚑', 'Review queue', VA.view.kind === 'review', REVIEW_QUEUE.length ? `<span class="chip warn">${REVIEW_QUEUE.length}</span>` : '')}
      </div>
      <div class="va-nav">
        <div class="nav-h">Sources</div>
        ${SOURCES.map(s => navItem('🗀', s.path, VA.view.kind === 'sources' && VA.view.src === s.id,
          s.status === 'indexing' ? '<span class="chip acc">indexing</span>' : '<span class="chip good">✓</span>')).join('')}
        <button class="nav-i" data-add-src><span class="lbl"><span class="nav-ic">+</span>Add folder…</span></button>
      </div>
      <div class="va-nav">
        <div class="nav-h">ROM Packs</div>
        ${PACKS.map(p => navItem('⬢', p.name, VA.view.kind === 'pack' && VA.view.key === p.id, p.setIds.length)).join('')}
        <button class="nav-i" data-new-pack><span class="lbl"><span class="nav-ic">+</span>New pack…</span></button>
      </div>
      <div class="va-nav">
        <div class="nav-h">Media Targets</div>
        ${TARGETS.map(t => navItem('▣', t.name, VA.view.kind === 'target' && VA.view.key === t.id,
          t.online ? '<span class="chip good">online</span>' : '<span class="chip bad">offline</span>')).join('')}
      </div>
    </aside>
    <main class="va-main" id="va-main"></main>
    <aside class="va-insp" id="va-insp"></aside>
  </div>
  <div id="va-modal-host"></div>`;

  // sidebar wiring
  const items = root.querySelectorAll('[data-nav]');
  const flat = [
    () => VA.view = { kind: 'platform', key: 'all' },
    ...platCounts.map(x => () => VA.view = { kind: 'platform', key: x.p.key }),
    () => VA.view = { kind: 'review' },
    ...SOURCES.map(s => () => VA.view = { kind: 'sources', src: s.id }),
    ...PACKS.map(p => () => VA.view = { kind: 'pack', key: p.id }),
    ...TARGETS.map(t => () => VA.view = { kind: 'target', key: t.id }),
  ];
  items.forEach((b, i) => b.onclick = () => { flat[i](); VA.selectedGame = null; renderVariantA(root); });
  root.querySelector('[data-add-src]').onclick = () => alert('Prototype: folder picker would open here.');
  root.querySelector('[data-new-pack]').onclick = () => {
    const name = prompt('Name the new ROM Pack:', 'Weekend Favorites');
    if (name) { PACKS.push({ id: 'p' + Date.now(), name, setIds: [] }); VA.view = { kind: 'pack', key: PACKS[PACKS.length - 1].id }; renderVariantA(root); }
  };

  vaMain(root);
  vaInspector(root);
  vaModal(root);
}

function vaMain(root) {
  const main = root.querySelector('#va-main');
  const v = VA.view;

  if (v.kind === 'platform') {
    const games = v.key === 'all' ? GAMES : GAMES.filter(g => g.platform === v.key);
    const grouped = PLATFORMS.map(p => ({ p, games: games.filter(g => g.platform === p.key) })).filter(x => x.games.length);
    const lensTarget = TARGETS.find(t => t.id === VA.lens);
    const dimmed = (g) => VA.lens && VA.filter !== 'all'
      && (VA.filter === 'on') !== gameOnTarget(g, VA.lens);
    const statusLine = (g) => {
      if (!VA.lens) return '';
      const inv = TARGET_INVENTORY[VA.lens];
      const on = setsOnTarget(g, VA.lens).length;
      if (!on) return '';
      const part = on < g.sets.length ? ` ${on} of ${g.sets.length} sets` : '';
      return `<div class="st" style="color:var(--${inv.stale ? 'warn' : 'good'})">● on ${esc(lensTarget.name)}${part}${inv.stale ? ' · last known' : ''}</div>`;
    };
    const statusText = (g) => { // compact, for list rows
      if (!VA.lens) return '';
      const inv = TARGET_INVENTORY[VA.lens];
      const on = setsOnTarget(g, VA.lens).length;
      if (!on) return '<span class="small" style="opacity:.5">— not on device</span>';
      const part = on < g.sets.length ? `${on}/${g.sets.length} sets ` : '';
      return `<span class="small" style="color:var(--${inv.stale ? 'warn' : 'good'})">● ${part}on device${inv.stale ? ' · last known' : ''}</span>`;
    };
    const gameRow = (g) => `
      <div class="va-listrow ${VA.selectedGame === g.id ? 'sel' : ''} ${dimmed(g) ? 'dim' : ''}" data-game="${g.id}">
        ${artTile(g, 'sm', { lens: VA.lens })}
        <div style="min-width:0"><b>${esc(g.title)}</b>${g.provisional ? ' <span class="chip warn">provisional</span>' : ''}
          <div class="small">${g.sets.map(s => esc(s.label)).join(' · ')}</div></div>
        <div>${statusText(g)}</div>
        <div class="small" style="text-align:right; white-space:nowrap">${g.sets.length} set${g.sets.length > 1 ? 's' : ''} · ${fmtSize(g.sets.reduce((n, s) => n + s.sizeMB, 0))}</div>
      </div>`;
    main.innerHTML = `
      <div class="va-toolbar">
        <input type="text" placeholder="Filter games…">
        <span class="seg" title="Device presence — show which games already live on a target">
          ${TARGETS.map(t => `<button data-lens="${t.id}" class="${VA.lens === t.id ? 'on' : ''}">◉ ${esc(t.name.split(' (')[0].split(' —')[0])}${TARGET_INVENTORY[t.id].stale ? ' · stale' : ''}</button>`).join('')}
          <button data-lens="" class="${VA.lens ? '' : 'on'}">presence off</button>
        </span>
        <span class="seg">
          ${[['all', 'All'], ['on', 'On device'], ['off', 'Not on device']].map(([k, l]) =>
            `<button data-filter="${k}" class="${VA.filter === k ? 'on' : ''}" ${VA.lens ? '' : 'disabled'}>${l}</button>`).join('')}
        </span>
        <span class="seg" title="View">
          <button data-vmode="grid" class="${VA.vmode === 'grid' ? 'on' : ''}">▦ Grid</button>
          <button data-vmode="list" class="${VA.vmode === 'list' ? 'on' : ''}">☰ List</button>
        </span>
        <span class="chip">Grouped by Platform</span>
        <span class="small">${games.length} games · ${games.reduce((n, g) => n + g.sets.length, 0)} ROM Sets</span>
      </div>
      ${grouped.map(x => `
        <div class="va-plat-h"><h2>${esc(x.p.name)}</h2><span class="chip acc">${esc(x.p.short)}</span><span class="small">${x.games.length}</span></div>
        ${VA.vmode === 'grid' ? `
        <div class="va-grid">
          ${x.games.map(g => `<div class="va-game ${VA.selectedGame === g.id ? 'sel' : ''} ${dimmed(g) ? 'dim' : ''}" data-game="${g.id}">
            ${artTile(g, '', { lens: VA.lens })}<div class="t">${esc(g.title)}</div>${statusLine(g)}</div>`).join('')}
        </div>` : `
        <div class="va-list">
          ${x.games.map(gameRow).join('')}
        </div>`}`).join('')}`;
    main.querySelectorAll('[data-game]').forEach(el => el.onclick = () => { VA.selectedGame = el.dataset.game; renderVariantA(root); });
    main.querySelectorAll('[data-vmode]').forEach(b => b.onclick = () => { VA.vmode = b.dataset.vmode; renderVariantA(root); });
    main.querySelectorAll('[data-lens]').forEach(b => b.onclick = () => { VA.lens = b.dataset.lens || null; renderVariantA(root); });
    main.querySelectorAll('[data-filter]').forEach(b => b.onclick = () => { VA.filter = b.dataset.filter; renderVariantA(root); });
    return;
  }

  if (v.kind === 'review') {
    main.innerHTML = `
      <h1>Review queue</h1>
      <p class="small">Uncertain matches accumulate here instead of interrupting import. Local identity stays usable either way.</p>
      ${REVIEW_QUEUE.map(r => `
        <div class="review-item">
          <div style="flex:1"><b class="mono">${esc(r.file)}</b><div class="small">Suggestion: ${esc(r.suggestion)}</div></div>
          <button data-rq-accept="${r.id}">Accept match</button>
          <button data-rq-search="${r.id}">Search again</button>
          <button class="ghost" data-rq-keep="${r.id}">Keep local</button>
        </div>`).join('')}`;
    main.querySelectorAll('[data-rq-accept],[data-rq-keep]').forEach(b => b.onclick = () => {
      const id = b.dataset.rqAccept || b.dataset.rqKeep;
      REVIEW_QUEUE.splice(REVIEW_QUEUE.findIndex(r => r.id === id), 1);
      renderVariantA(root);
    });
    main.querySelectorAll('[data-rq-search]').forEach(b => b.onclick = () => alert('Prototype: provider search dialog.'));
    return;
  }

  if (v.kind === 'sources') {
    main.innerHTML = `
      <h1>Sources</h1>
      <p class="small">Indexing reads bytes to establish content identity — hashes never leave the app except minimum provider lookups. Re-index anytime; Library identity is stable.</p>
      ${SOURCES.map(s => `
        <div class="review-item">
          <div style="flex:1"><b class="mono">${esc(s.path)}</b>
            <div class="small">${s.status === 'indexing' ? esc(s.note) : `${s.files.toLocaleString()} files scanned · ${s.matched.toLocaleString()} recognized ROMs${s.note ? ' · ' + esc(s.note) : ''}`}</div></div>
          ${s.status === 'indexing' ? '<span class="chip acc">indexing…</span>' : '<button data-reindex>Re-index</button>'}
        </div>`).join('')}
      <button>+ Add folder…</button>`;
    main.querySelectorAll('[data-reindex]').forEach(b => b.onclick = () => alert('Prototype: re-index started.'));
    return;
  }

  if (v.kind === 'pack') {
    const pack = PACKS.find(p => p.id === v.key);
    const sets = packSets(pack);
    main.innerHTML = `
      <div class="va-toolbar">
        <h1 style="flex:1">${esc(pack.name)}</h1>
        <span class="chip">${sets.length} ROM Sets · ${fmtSize(packSizeMB(pack))}</span>
        <button class="primary" data-sync-pack>Sync to target…</button>
      </div>
      ${sets.length === 0 ? '<p class="muted">Empty pack. Add games from the Library.</p>' : ''}
      <div class="va-grid">
        ${sets.map(x => `<div class="va-game" data-game="${x.game.id}">${artTile(x.game, '', { lens: VA.lens })}<div class="t">${esc(x.game.title)}<br><span class="small">${esc(x.set.label)}</span></div></div>`).join('')}
      </div>`;
    main.querySelectorAll('[data-game]').forEach(el => el.onclick = () => { VA.selectedGame = el.dataset.game; renderVariantA(root); });
    main.querySelector('[data-sync-pack]').onclick = () => { VA.wizard = { step: 2, packId: pack.id, targetId: null, plan: null, planSt: { ack: false, conflictResolved: false }, scenario: 'normal' }; renderVariantA(root); };
    return;
  }

  if (v.kind === 'target') {
    const t = TARGETS.find(x => x.id === v.key);
    main.innerHTML = `
      <h1>${esc(t.name)}</h1>
      <div class="plan-meta" style="margin-top:12px">
        <div class="m"><div class="k">Status</div>${t.online ? '<span class="chip good">online</span>' : '<span class="chip bad">offline</span>'}<div class="small">${esc(t.scan)}</div></div>
        <div class="m"><div class="k">Device Profile</div>${esc(t.profile)}</div>
        <div class="m"><div class="k">Binding</div>${esc(t.binding)}</div>
        <div class="m"><div class="k">Marker</div><span class="mono">${esc(t.marker)}</span><div class="small">${t.managed} managed artifacts</div></div>
      </div>
      ${t.online
        ? `<h3 style="margin-top:18px">Sync a pack here</h3>
           ${PACKS.map(p => `<div class="review-item"><div style="flex:1"><b>${esc(p.name)}</b><div class="small">${p.setIds.length} ROM Sets · ${fmtSize(packSizeMB(p))}</div></div><button class="primary" data-sync-tp="${p.id}">Review Sync Plan…</button></div>`).join('')}`
        : `<div class="banner" style="margin-top:18px"><b>Offline.</b> Last-known inventory is stale. You can prepare packs now; an executable Sync Plan needs a current scan after reconnect.</div>`}`;
    main.querySelectorAll('[data-sync-tp]').forEach(b => b.onclick = () => {
      VA.wizard = { step: 3, packId: b.dataset.syncTp, targetId: t.id, plan: buildPlan(), planSt: { ack: false, conflictResolved: false }, scenario: 'normal' };
      renderVariantA(root);
    });
    return;
  }
}

function vaInspector(root) {
  const insp = root.querySelector('#va-insp');
  const g = GAMES.find(x => x.id === VA.selectedGame);
  if (!g) { insp.innerHTML = `<h3>Inspector</h3><p class="small muted">Select a game to see its Releases, ROM Sets, and pack membership.</p>`; return; }
  const inPacks = PACKS.filter(p => g.sets.some(s => p.setIds.includes(s.id)));
  insp.innerHTML = `
    ${artTile(g, '', { lens: VA.lens })}
    <h2 style="margin-top:10px">${esc(g.title)}</h2>
    <div class="small">${esc(platformOf(g.platform).name)} ${g.provisional ? '· <span class="chip warn">provisional — locally identified</span>' : '· <span class="chip good">catalog matched</span>'}</div>
    <h3 style="margin-top:14px">ROM Sets</h3>
    ${g.sets.map(s => `
      <div class="va-setrow">
        <div style="display:flex; justify-content:space-between; align-items:center">
          <b class="small">${esc(s.label)}</b>
          ${s.complete ? '<span class="chip good">complete</span>' : `<span class="chip bad">incomplete — ${s.files.length} of ${s.expected}</span>`}
        </div>
        <div class="small mono">${s.files.length} file(s) · ${fmtSize(s.sizeMB)}</div>
        ${s.hack ? '<div class="small">Derived release — base + patch lineage recorded.</div>' : ''}
        <div style="margin-top:7px; display:flex; gap:5px; flex-wrap:wrap; align-items:center">
          <span class="small" style="color:var(--faint)">On device:</span>
          ${TARGETS.map(t => {
            const inv = TARGET_INVENTORY[t.id];
            const on = inv.sets.includes(s.id);
            return on
              ? `<span class="chip ${inv.stale ? 'warn' : 'good'}" title="${esc(inv.scan)}">● ${esc(inv.short)}${inv.stale ? ' · last known' : ''}</span>`
              : `<span class="chip" style="opacity:.55">— ${esc(inv.short)}</span>`;
          }).join('')}
        </div>
        <div style="margin-top:7px; display:flex; gap:6px; align-items:center">
          ${s.complete ? `<select data-pack-for="${s.id}">
              <option value="">Add to pack…</option>${PACKS.map(p => `<option value="${p.id}">${esc(p.name)}</option>`).join('')}</select>`
            : '<span class="small muted">Incomplete sets can’t enter a ROM Pack</span>'}
        </div>
      </div>`).join('')}
    <h3 style="margin-top:10px">In packs</h3>
    ${inPacks.length ? inPacks.map(p => `<span class="chip acc" style="margin-right:4px">${esc(p.name)}</span>`).join('') : '<span class="small muted">none</span>'}`;
  insp.querySelectorAll('[data-pack-for]').forEach(sel => sel.onchange = () => {
    const pack = PACKS.find(p => p.id === sel.value);
    if (pack && !pack.setIds.includes(sel.dataset.packFor)) pack.setIds.push(sel.dataset.packFor);
    renderVariantA(root);
  });
}

/* modal wizard: Pack → Target → Plan → Run */
function vaModal(root) {
  const host = root.querySelector('#va-modal-host');
  const w = VA.wizard;
  if (!w) { host.innerHTML = ''; return; }
  const steps = ['Pack', 'Target', 'Sync Plan', 'Run'];

  host.innerHTML = `
    <div class="va-modal-veil">
      <div class="va-modal">
        <div style="display:flex; align-items:center; margin-bottom:6px">
          <h2 style="flex:1">Sync to a Media Target</h2>
          <button class="ghost" data-wz-close>✕</button>
        </div>
        <div class="va-steps">${steps.map((s, i) => `<div class="st ${w.step === i + 1 ? 'on' : ''}">${i + 1}. ${s}</div>`).join('')}</div>
        <div id="wz-body"></div>
      </div>
    </div>`;
  host.querySelector('[data-wz-close]').onclick = () => { VA.wizard = null; renderVariantA(root); };
  const body = host.querySelector('#wz-body');

  if (w.step === 2) {
    body.innerHTML = `
      <h3>Choose a Media Target for “${esc(PACKS.find(p => p.id === w.packId).name)}”</h3>
      ${TARGETS.map(t => {
        const pack = PACKS.find(p => p.id === w.packId);
        const onCount = pack.setIds.filter(id => TARGET_INVENTORY[t.id].sets.includes(id)).length;
        return `
        <div class="review-item">
          <div style="flex:1"><b>${esc(t.name)}</b><div class="small">${esc(t.profile)} · ${esc(t.binding)}</div>
            <div class="small" style="margin-top:3px">${onCount ? `<span style="color:var(--${TARGET_INVENTORY[t.id].stale ? 'warn' : 'good'})">● ${onCount} of ${pack.setIds.length} sets already on this target</span>${TARGET_INVENTORY[t.id].stale ? ' (last known)' : ''}` : 'none of this pack on this target yet'}</div></div>
          ${t.online ? `<button class="primary" data-wz-target="${t.id}">Build Sync Plan</button>` : '<span class="chip bad">offline — needs current scan</span>'}
        </div>`;
      }).join('')}`;
    body.querySelectorAll('[data-wz-target]').forEach(b => b.onclick = () => {
      w.targetId = b.dataset.wzTarget; w.plan = buildPlan(); w.step = 3; renderVariantA(root);
    });
    return;
  }

  if (w.step === 3) {
    body.innerHTML = `<h3>Inspect the Sync Plan — immutable snapshot</h3><div id="wz-plan"></div>`;
    renderPlanInto(body.querySelector('#wz-plan'), w.plan, w.planSt, { onApprove: () => { w.step = 4; renderVariantA(root); } });
    return;
  }

  if (w.step === 4) {
    body.innerHTML = `
      <div class="va-toolbar">
        <h3 style="flex:1">Running — ${esc(w.plan.pack)} → ${esc(w.plan.target.name)}</h3>
        <label class="small"><input type="checkbox" data-wz-disc ${w.scenario === 'disconnect' ? 'checked' : ''}> simulate unplug mid-transfer</label>
      </div>
      <div id="wz-prog"></div>`;
    body.querySelector('[data-wz-disc]').onchange = (e) => { w.scenario = e.target.checked ? 'disconnect' : 'normal'; renderVariantA(root); };
    runProgressInto(body.querySelector('#wz-prog'), w.plan, {
      disconnectAt: w.scenario === 'disconnect' ? 3 : null,
      onRefresh: () => { w.step = 3; w.plan = buildPlan(); w.planSt = { ack: false, conflictResolved: true }; renderVariantA(root); },
    });
    return;
  }
}
