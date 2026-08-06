/* THROWAWAY PROTOTYPE — Variant C: "Pack Home".
   The ROM Pack is the hero object. Home is pack cards with per-target sync
   status. Plan review and progress live inline on the pack page, next to the
   contents they affect. Library and Sources/Targets are secondary tabs. */

const VC = {
  tab: 'packs',        // packs | library | sys
  packId: null,        // open pack page
  drawer: false,       // add-games drawer open
  sync: {},            // packId -> {open, plan, planSt, running, scenario}
};

function renderVariantC(root) {
  root.innerHTML = `
  <div class="vc">
    <div class="vc-top">
      <h1>ROM Manager</h1>
      <button class="vc-tab ${VC.tab === 'packs' ? 'on' : ''}" data-tab="packs">ROM Packs</button>
      <button class="vc-tab ${VC.tab === 'library' ? 'on' : ''}" data-tab="library">Library <span class="chip">${GAMES.length}</span></button>
      <button class="vc-tab ${VC.tab === 'sys' ? 'on' : ''}" data-tab="sys">Sources &amp; Targets</button>
      ${REVIEW_QUEUE.length ? `<span style="flex:1"></span><span class="chip warn">${REVIEW_QUEUE.length} matches to review</span>` : ''}
    </div>
    <div class="vc-main" id="vc-main"></div>
  </div>
  <div id="vc-drawer-host"></div>`;
  root.querySelectorAll('[data-tab]').forEach(b => b.onclick = () => { VC.tab = b.dataset.tab; VC.packId = null; VC.drawer = false; renderVariantC(root); });
  vcMain(root);
  vcDrawer(root);
}

function vcMain(root) {
  const main = root.querySelector('#vc-main');

  /* ---------------- Packs home ---------------- */
  if (VC.tab === 'packs' && !VC.packId) {
    main.innerHTML = `
      <h1>Your ROM Packs</h1>
      <p class="muted">Each pack knows how it stands against every target you've synced it to.</p>
      <div class="vc-packs">
        ${PACKS.map(p => {
          const sets = packSets(p);
          return `<div class="vc-packcard" data-open-pack="${p.id}">
            <h2>${esc(p.name)}</h2>
            <div class="vc-strip">${sets.slice(0, 7).map(x => artTile(x.game, 'sm')).join('')}${sets.length > 7 ? `<div class="art sm" style="--hue:220;background:var(--bg3)"><span>+${sets.length - 7}</span></div>` : ''}</div>
            <div class="small">${sets.length} ROM Sets · ${fmtSize(packSizeMB(p))}</div>
            <div style="margin-top:8px; display:flex; gap:6px; flex-wrap:wrap">
              <span class="chip warn">Odin 3 SD: 6 to add · 3 to remove</span>
              <span class="chip">Internal: offline</span>
            </div>
          </div>`;
        }).join('')}
        <div class="vc-packcard" style="border-style:dashed; display:flex; align-items:center; justify-content:center; min-height:140px">
          <button data-new-pack>+ New pack</button>
        </div>
      </div>`;
    main.querySelectorAll('[data-open-pack]').forEach(c => c.onclick = () => { VC.packId = c.dataset.openPack; renderVariantC(root); });
    main.querySelector('[data-new-pack]').onclick = () => {
      const name = prompt('Name the new ROM Pack:', 'Shmup Sunday');
      if (name) { PACKS.push({ id: 'p' + Date.now(), name, setIds: [] }); renderVariantC(root); }
    };
    return;
  }

  /* ---------------- Pack page ---------------- */
  if (VC.tab === 'packs' && VC.packId) {
    const p = PACKS.find(x => x.id === VC.packId);
    const sets = packSets(p);
    const sync = VC.sync[p.id] || (VC.sync[p.id] = { open: false, plan: null, planSt: { ack: false, conflictResolved: false }, scenario: 'normal' });
    main.innerHTML = `
      <div class="vc-packhead">
        <button class="ghost" data-back>← Packs</button>
        <h1 style="flex:1; margin:0">${esc(p.name)}</h1>
        <span class="chip">${sets.length} ROM Sets · ${fmtSize(packSizeMB(p))}</span>
        <button class="primary" data-add-games>+ Add games</button>
      </div>
      <div class="vc-panel" style="margin-top:0">
        <h3>Contents</h3>
        ${sets.length === 0 ? '<p class="muted">Nothing here yet — add games from the Library.</p>' : ''}
        ${sets.map(x => `
          <div class="vc-setline">
            ${artTile(x.game, 'sm')}
            <div style="flex:1; min-width:0">
              <b>${esc(x.game.title)}</b> <span class="chip">${esc(platformOf(x.game.platform).short)}</span>
              <div class="small">${esc(x.set.label)} · ${fmtSize(x.set.sizeMB)}${x.set.hack ? ' · derived release (base + patch)' : ''}</div>
            </div>
            <button class="ghost" data-remove-set="${x.set.id}" title="Remove from pack">✕</button>
          </div>`).join('')}
      </div>
      <div class="vc-panel">
        <div style="display:flex; align-items:center; gap:10px">
          <h3 style="flex:1; margin:0">Targets</h3>
        </div>
        ${TARGETS.map(t => `
          <div class="vc-setline">
            <div style="flex:1">
              <b>${esc(t.name)}</b> ${t.online ? '<span class="chip good">online</span>' : '<span class="chip bad">offline</span>'}
              <div class="small">${esc(t.profile)} · ${esc(t.binding)} · ${esc(t.scan)}</div>
            </div>
            ${t.online
              ? `<button data-plan-toggle="${t.id}">${sync.open ? 'Hide Sync Plan' : 'Review Sync Plan'}</button>`
              : '<span class="small muted">connect to plan</span>'}
          </div>
          ${t.online && sync.open ? '<div id="vc-plan-host" style="padding:8px 4px 4px"></div>' : ''}`).join('')}
      </div>`;
    main.querySelector('[data-back]').onclick = () => { VC.packId = null; renderVariantC(root); };
    main.querySelector('[data-add-games]').onclick = () => { VC.drawer = true; renderVariantC(root); };
    main.querySelectorAll('[data-remove-set]').forEach(b => b.onclick = () => {
      p.setIds = p.setIds.filter(id => id !== b.dataset.removeSet);
      sync.plan = null; sync.open = false;
      renderVariantC(root);
    });
    main.querySelectorAll('[data-plan-toggle]').forEach(b => b.onclick = () => {
      sync.open = !sync.open;
      if (sync.open && !sync.plan) { sync.plan = buildPlan(); sync.planSt = { ack: false, conflictResolved: false }; }
      renderVariantC(root);
    });
    const planHost = main.querySelector('#vc-plan-host');
    if (planHost && sync.open) {
      if (!sync.running) {
        planHost.innerHTML = `
          <div class="small" style="margin-bottom:8px">
            <label><input type="checkbox" data-disc ${sync.scenario === 'disconnect' ? 'checked' : ''}> simulate unplug mid-transfer</label>
          </div>
          <div id="vc-plan-doc"></div>`;
        planHost.querySelector('[data-disc]').onchange = (e) => { sync.scenario = e.target.checked ? 'disconnect' : 'normal'; };
        renderPlanInto(planHost.querySelector('#vc-plan-doc'), sync.plan, sync.planSt, {
          onApprove: () => { sync.running = true; renderVariantC(root); },
        });
      } else {
        runProgressInto(planHost, sync.plan, {
          disconnectAt: sync.scenario === 'disconnect' ? 3 : null,
          onDone: () => {},
          onRefresh: () => { sync.running = false; sync.plan = buildPlan(); sync.planSt = { ack: false, conflictResolved: true }; renderVariantC(root); },
        });
      }
    }
    return;
  }

  /* ---------------- Library tab (secondary) ---------------- */
  if (VC.tab === 'library') {
    const grouped = PLATFORMS.map(p => ({ p, games: GAMES.filter(g => g.platform === p.key) })).filter(x => x.games.length);
    main.innerHTML = `
      <h1>Library</h1>
      ${REVIEW_QUEUE.length ? `<div class="banner" style="margin:10px 0 16px">
        ${REVIEW_QUEUE.map(r => `<div style="display:flex; gap:10px; align-items:center; padding:3px 0">
          <span class="mono" style="flex:1">${esc(r.file)}</span><span class="small">${esc(r.suggestion)}</span>
          <button data-rq="${r.id}">Accept</button><button class="ghost" data-rq="${r.id}">Keep local</button></div>`).join('')}
      </div>` : ''}
      ${grouped.map(x => `
        <h3 style="margin-top:18px">${esc(x.p.name)}</h3>
        <div class="va-grid">${x.games.map(g => `<div class="va-game">${artTile(g)}<div class="t">${esc(g.title)}</div></div>`).join('')}</div>`).join('')}`;
    main.querySelectorAll('[data-rq]').forEach(b => b.onclick = () => {
      REVIEW_QUEUE.splice(REVIEW_QUEUE.findIndex(r => r.id === b.dataset.rq), 1);
      renderVariantC(root);
    });
    return;
  }

  /* ---------------- Sources & Targets tab ---------------- */
  if (VC.tab === 'sys') {
    main.innerHTML = `
      <h1>Sources &amp; Targets</h1>
      <div class="vc-panel" style="margin-top:14px">
        <h3>Source folders</h3>
        ${SOURCES.map(s => `<div class="vc-setline"><div style="flex:1"><b class="mono">${esc(s.path)}</b>
          <div class="small">${s.status === 'indexing' ? esc(s.note) : s.files.toLocaleString() + ' files · ' + s.matched.toLocaleString() + ' recognized ROMs'}</div></div>
          ${s.status === 'indexing' ? '<span class="chip acc">indexing</span>' : '<span class="chip good">indexed</span>'}</div>`).join('')}
        <button style="margin-top:8px">+ Add folder…</button>
      </div>
      <div class="vc-panel">
        <h3>Media Targets</h3>
        ${TARGETS.map(t => `<div class="vc-setline"><div style="flex:1"><b>${esc(t.name)}</b>
          <div class="small">${esc(t.profile)} · ${esc(t.binding)} · marker <span class="mono">${esc(t.marker)}</span></div></div>
          ${t.online ? '<span class="chip good">online</span>' : '<span class="chip bad">offline</span>'}</div>`).join('')}
        <p class="small" style="margin-top:8px">First-time binding picks a storage root and confirms a Device Profile — sync never starts by itself.</p>
      </div>`;
    return;
  }
}

/* add-games drawer on the pack page */
function vcDrawer(root) {
  const host = root.querySelector('#vc-drawer-host');
  if (!VC.drawer || !VC.packId) { host.innerHTML = ''; return; }
  const p = PACKS.find(x => x.id === VC.packId);
  host.innerHTML = `
    <div class="vc-drawer-veil" data-drawer-close></div>
    <div class="vc-drawer">
      <div style="display:flex; align-items:center; margin-bottom:12px">
        <h2 style="flex:1">Add games to “${esc(p.name)}”</h2>
        <button class="ghost" data-drawer-close>✕</button>
      </div>
      ${PLATFORMS.map(pl => {
        const rows = GAMES.filter(g => g.platform === pl.key)
          .flatMap(g => g.sets.map(s => ({ g, s })));
        if (!rows.length) return '';
        return `<h3 style="margin-top:14px">${esc(pl.name)}</h3>` + rows.map(x => `
          <label style="display:flex; gap:10px; align-items:center; padding:5px 0" class="small">
            <input type="checkbox" data-addset="${x.s.id}" ${p.setIds.includes(x.s.id) ? 'checked' : ''} ${x.s.complete ? '' : 'disabled'}>
            ${artTile(x.g, 'sm')}
            <span style="flex:1">${esc(x.g.title)}<br><span class="muted">${esc(x.s.label)}${x.s.complete ? '' : ' — incomplete, can’t be packed'}</span></span>
          </label>`).join('');
      }).join('')}
    </div>`;
  host.querySelectorAll('[data-drawer-close]').forEach(b => b.onclick = () => { VC.drawer = false; renderVariantC(root); });
  host.querySelectorAll('[data-addset]').forEach(cb => cb.onchange = () => {
    if (cb.checked) p.setIds.push(cb.dataset.addset);
    else p.setIds = p.setIds.filter(id => id !== cb.dataset.addset);
    const sync = VC.sync[p.id];
    if (sync) { sync.plan = null; sync.open = false; sync.running = false; }
    renderVariantC(root); // VC.drawer is still true, so the drawer stays open
  });
}
