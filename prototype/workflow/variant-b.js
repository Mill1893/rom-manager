/* THROWAWAY PROTOTYPE — Variant B: "Guided Flow".
   The app is organized as the pipeline itself: Sources → Library → Packs →
   Sync → Activity. Each stage is one focused full-width screen with an
   obvious next step. Sync is a linear stepper; the plan is a full page;
   errors get full-page attention states. */

const VB = {
  stage: 'sources',
  sync: null,
  activity: [],
};

const VB_STAGES = [
  ['sources', 'Sources', 'Index the folders your ROMs live in'],
  ['library', 'Library', 'Browse and group your games'],
  ['packs', 'ROM Packs', 'Curate reusable selections'],
  ['sync', 'Sync', 'Plan and transfer to a device'],
  ['activity', 'Activity', 'Operations, progress, and attention'],
];

function renderVariantB(root) {
  const attention = VB.activity.filter(a => a.needsYou).length;
  root.innerHTML = `
  <div class="vb">
    <aside class="vb-rail">
      <h1>ROM Manager</h1>
      ${VB_STAGES.map(([k, label, desc], i) => `
        <button class="vb-step ${VB.stage === k ? 'on' : ''}" data-stage="${k}">
          <span class="n">${k === 'activity' && attention ? '!' : i + 1}</span>
          <span><div>${esc(label)} ${k === 'activity' && attention ? `<span class="chip bad">${attention}</span>` : ''}</div>
          <div class="desc">${esc(desc)}</div></span>
        </button>`).join('')}
    </aside>
    <main class="vb-main" id="vb-main"></main>
  </div>`;
  root.querySelectorAll('[data-stage]').forEach(b => b.onclick = () => { VB.stage = b.dataset.stage; renderVariantB(root); });
  vbStage(root);
}

function vbNext(main, label, stage) {
  const host = main.querySelector('#vb-next-host');
  if (host) {
    host.innerHTML = `<button class="primary" data-next="${stage}">${esc(label)} →</button>`;
    host.querySelector('[data-next]').onclick = () => { VB.stage = stage; renderVariantB(document.getElementById('root')); };
  }
}

function vbStage(root) {
  const main = root.querySelector('#vb-main');

  /* ---------------- 1. Sources ---------------- */
  if (VB.stage === 'sources') {
    main.innerHTML = `
      <div class="vb-hero"><h1>Where do your ROMs live?</h1>
      <p class="muted">Add every folder you keep games in. Indexing reads file bytes to establish content identity — nothing is uploaded except minimal provider lookups.</p></div>
      <div class="vb-cards">
        ${SOURCES.map(s => `
          <div class="vb-card">
            <b class="mono">${esc(s.path)}</b>
            <div class="small" style="margin:6px 0">${s.status === 'indexing' ? esc(s.note) : s.files.toLocaleString() + ' files · ' + s.matched.toLocaleString() + ' recognized ROMs'}</div>
            ${s.status === 'indexing'
              ? '<div class="prog-bar"><div style="width:41%"></div></div><div class="small" style="margin-top:4px">Indexing — you can keep working</div>'
              : '<span class="chip good">indexed</span>' + (s.note ? '<div class="small" style="margin-top:6px">' + esc(s.note) + '</div>' : '')}
          </div>`).join('')}
        <div class="vb-card" style="display:flex; align-items:center; justify-content:center; border-style:dashed">
          <button data-add-src>+ Add a folder</button>
        </div>
      </div>
      <div id="vb-next-host" style="margin-top:26px"></div>`;
    main.querySelector('[data-add-src]').onclick = () => alert('Prototype: folder picker.');
    vbNext(main, 'Continue to your Library', 'library');
    return;
  }

  /* ---------------- 2. Library ---------------- */
  if (VB.stage === 'library') {
    const grouped = PLATFORMS.map(p => ({ p, games: GAMES.filter(g => g.platform === p.key) })).filter(x => x.games.length);
    main.innerHTML = `
      <div class="vb-hero"><h1>Your Library</h1>
      <p class="muted">${GAMES.length} games across ${grouped.length} Platforms. Grouped by Platform — every playable form of a title stays on one row.</p></div>
      ${REVIEW_QUEUE.length ? `
        <div class="banner" style="margin-bottom:18px; display:flex; align-items:center; gap:10px">
          <span style="flex:1"><b>${REVIEW_QUEUE.length} matches need your call.</b> Import never blocks on uncertain metadata — review whenever.</span>
          <button data-rq-open>Review now</button>
        </div>` : ''}
      <div id="vb-rq" style="display:none; margin-bottom:18px">
        ${REVIEW_QUEUE.map(r => `
          <div class="review-item">
            <div style="flex:1"><b class="mono">${esc(r.file)}</b><div class="small">${esc(r.suggestion)}</div></div>
            <button data-rq-done="${r.id}">Accept</button><button class="ghost" data-rq-done="${r.id}">Keep local</button>
          </div>`).join('')}
      </div>
      ${grouped.map(x => `
        <h3 style="margin-top:18px">${esc(x.p.name)} <span class="chip">${x.games.length}</span></h3>
        ${x.games.map(g => `
          <div class="vb-row">
            ${artTile(g, 'sm')}
            <div class="grow"><b>${esc(g.title)}</b>
              <div class="small">${g.sets.map(s => s.complete ? esc(s.label) : '⚠ ' + esc(s.label) + ' — incomplete').join(' · ')}</div></div>
            ${g.provisional ? '<span class="chip warn">provisional</span>' : '<span class="chip good">matched</span>'}
          </div>`).join('')}`).join('')}
      <div id="vb-next-host" style="margin-top:26px"></div>`;
    const openBtn = main.querySelector('[data-rq-open]');
    if (openBtn) openBtn.onclick = () => {
      const rq = main.querySelector('#vb-rq');
      rq.style.display = rq.style.display === 'none' ? 'block' : 'none';
    };
    main.querySelectorAll('[data-rq-done]').forEach(b => b.onclick = () => {
      REVIEW_QUEUE.splice(REVIEW_QUEUE.findIndex(r => r.id === b.dataset.rqDone), 1);
      renderVariantB(root);
    });
    vbNext(main, 'Build your ROM Packs', 'packs');
    return;
  }

  /* ---------------- 3. Packs ---------------- */
  if (VB.stage === 'packs') {
    main.innerHTML = `
      <div class="vb-hero"><h1>ROM Packs</h1>
      <p class="muted">Reusable selections of exact, complete ROM Sets. Dependencies come along automatically.</p></div>
      ${PACKS.map(p => {
        const sets = packSets(p);
        return `<div class="vb-card" style="margin-bottom:12px">
          <div style="display:flex; align-items:center; gap:10px">
            <h3 style="flex:1; margin:0">${esc(p.name)}</h3>
            <span class="chip">${sets.length} sets · ${fmtSize(packSizeMB(p))}</span>
            <button data-edit-pack="${p.id}">Edit contents</button>
          </div>
          <div class="small" style="margin-top:6px">${sets.map(x => esc(x.game.title)).join(' · ') || 'empty'}</div>
          <div data-pack-editor="${p.id}" style="display:none; margin-top:12px; border-top:1px solid var(--line); padding-top:12px">
            ${GAMES.flatMap(g => g.sets.filter(s => s.complete).map(s => ({ g, s }))).map(x => `
              <label style="display:flex; gap:8px; align-items:center; padding:3px 0" class="small">
                <input type="checkbox" data-pack-toggle="${p.id}:${x.s.id}" ${p.setIds.includes(x.s.id) ? 'checked' : ''}>
                <span>${esc(x.g.title)} — <span class="muted">${esc(x.s.label)}</span></span>
                <span class="chip">${esc(platformOf(x.g.platform).short)}</span>
              </label>`).join('')}
          </div>
        </div>`;
      }).join('')}
      <button data-new-pack>+ New pack</button>
      <div id="vb-next-host" style="margin-top:26px"></div>`;
    main.querySelectorAll('[data-edit-pack]').forEach(b => b.onclick = () => {
      const ed = main.querySelector('[data-pack-editor="' + b.dataset.editPack + '"]');
      ed.style.display = ed.style.display === 'none' ? 'block' : 'none';
    });
    main.querySelectorAll('[data-pack-toggle]').forEach(cb => cb.onchange = () => {
      const parts = cb.dataset.packToggle.split(':');
      const p = PACKS.find(x => x.id === parts[0]);
      if (cb.checked) p.setIds.push(parts[1]); else p.setIds = p.setIds.filter(id => id !== parts[1]);
      renderVariantB(root);
      const ed = root.querySelector('[data-pack-editor="' + parts[0] + '"]');
      if (ed) ed.style.display = 'block';
    });
    main.querySelector('[data-new-pack]').onclick = () => {
      const name = prompt('Name the new ROM Pack:', 'Arcade Night');
      if (name) { PACKS.push({ id: 'p' + Date.now(), name, setIds: [] }); renderVariantB(root); }
    };
    vbNext(main, 'Sync a pack to a device', 'sync');
    return;
  }

  vbSyncStage(root, main);
}

/* ---------------- 4. Sync (linear full-page stepper) ---------------- */
function vbSyncStage(root, main) {
  if (VB.stage === 'activity') { vbActivityStage(root, main); return; }
  if (!VB.sync) VB.sync = { step: 'pack', packId: null, targetId: null, plan: null, planSt: { ack: false, conflictResolved: false }, scenario: 'normal' };
  const s = VB.sync;
  const steps = [['pack', 'Choose a pack'], ['target', 'Choose a target'], ['plan', 'Review the plan'], ['run', 'Transfer']];
  const idx = steps.findIndex(x => x[0] === s.step);
  const head = `
    <div class="vb-hero"><h1>Sync to a device</h1></div>
    <div class="vb-stepper">${steps.map(([k, label], i) =>
      `<div class="st ${i < idx ? 'done' : ''} ${i === idx ? 'on' : ''}">${i + 1}. ${esc(label)}</div>`).join('')}</div>`;

  if (s.step === 'pack') {
    main.innerHTML = head + `
      <h2>Which ROM Pack goes to the device?</h2>
      <div class="vb-pickgrid">
        ${PACKS.map(p => `
          <button class="vb-pick ${s.packId === p.id ? 'sel' : ''}" data-pick-pack="${p.id}">
            <b>${esc(p.name)}</b>
            <div class="small" style="margin-top:4px">${p.setIds.length} ROM Sets · ${fmtSize(packSizeMB(p))}</div>
          </button>`).join('')}
      </div>
      <div style="margin-top:22px"><button class="primary" data-go="target" ${s.packId ? '' : 'disabled'}>Continue →</button></div>`;
    main.querySelectorAll('[data-pick-pack]').forEach(b => b.onclick = () => { s.packId = b.dataset.pickPack; renderVariantB(root); });
  }

  if (s.step === 'target') {
    main.innerHTML = head + `
      <h2>Which Media Target?</h2>
      <div class="vb-pickgrid">
        ${TARGETS.map(t => `
          <button class="vb-pick ${s.targetId === t.id ? 'sel' : ''}" data-pick-target="${t.id}" ${t.online ? '' : 'disabled'}>
            <b>${esc(t.name)}</b>
            <div class="small" style="margin-top:4px">${esc(t.profile)} · ${esc(t.binding)}</div>
            <div style="margin-top:6px">${t.online ? '<span class="chip good">online — scan current</span>' : '<span class="chip bad">offline — connect to plan</span>'}</div>
          </button>`).join('')}
      </div>
      <p class="small" style="margin-top:10px">A target is one physical storage root — the same SD card reached by card reader or MTP stays one target.</p>
      <div style="margin-top:12px; display:flex; gap:8px">
        <button data-go="pack">← Back</button>
        <button class="primary" data-go="plan" ${s.targetId ? '' : 'disabled'}>Build the Sync Plan →</button>
      </div>`;
    main.querySelectorAll('[data-pick-target]').forEach(b => b.onclick = () => { s.targetId = b.dataset.pickTarget; renderVariantB(root); });
  }

  if (s.step === 'plan') {
    if (!s.plan) s.plan = buildPlan();
    main.innerHTML = head + `
      <h2>Review the Sync Plan</h2>
      <p class="muted">An immutable snapshot. Approving it — once — covers every action listed, including the named permanent removals.</p>
      <div id="vb-plan"></div>
      <div style="margin-top:12px"><button data-go="target">← Back</button></div>`;
    renderPlanInto(main.querySelector('#vb-plan'), s.plan, s.planSt, { onApprove: () => { s.step = 'run'; renderVariantB(root); } });
  }

  if (s.step === 'run') {
    main.innerHTML = head + `
      <h2>Transferring — ${esc(s.plan.pack)} → ${esc(s.plan.target.name)}</h2>
      <div class="small" style="margin-bottom:10px">
        <label><input type="checkbox" data-disc ${s.scenario === 'disconnect' ? 'checked' : ''}> simulate unplug mid-transfer</label>
      </div>
      <div id="vb-prog"></div>`;
    const d = main.querySelector('[data-disc]');
    d.onchange = () => { s.scenario = d.checked ? 'disconnect' : 'normal'; renderVariantB(root); };
    runProgressInto(main.querySelector('#vb-prog'), s.plan, {
      disconnectAt: s.scenario === 'disconnect' ? 3 : null,
      onDone: (sim) => {
        VB.activity = VB.activity.filter(a => a.kind !== 'sync');
        VB.activity.unshift(sim.phase === 'done'
          ? { kind: 'sync', title: 'Synced “' + s.plan.pack + '” → ' + s.plan.target.name, detail: 'All artifacts placed and verified.', needsYou: false }
          : { kind: 'sync', title: 'Sync ' + (sim.cancelled ? 'cancelled' : 'interrupted') + ' — ' + s.plan.pack, detail: 'Destination indeterminate. Refresh and confirm a new plan.', needsYou: true });
      },
      onRefresh: () => { s.step = 'plan'; s.plan = buildPlan(); s.planSt = { ack: false, conflictResolved: true }; renderVariantB(root); },
    });
  }

  main.querySelectorAll('[data-go]').forEach(b => b.onclick = () => {
    if (b.dataset.go === 'plan' && !s.plan) s.plan = buildPlan();
    s.step = b.dataset.go;
    renderVariantB(root);
  });
}

/* ---------------- 5. Activity ---------------- */
function vbActivityStage(root, main) {
  const items = VB.activity.length ? VB.activity : [
    { kind: 'index', title: 'Indexed ~/ROMs/No-Intro', detail: '16,411 files · 1,587 recognized ROMs · finished 09:12', needsYou: false },
    { kind: 'sync', title: 'No sync operations yet', detail: 'Approved plans and their outcomes will journal here.', needsYou: false },
  ];
  main.innerHTML = `
    <div class="vb-hero"><h1>Activity</h1>
    <p class="muted">The durable operation journal. Nothing here ever resumes automatically — interrupted work needs a refresh and a fresh plan.</p></div>
    ${items.map(a => `
      <div class="review-item">
        <div style="flex:1"><b>${esc(a.title)}</b><div class="small">${esc(a.detail)}</div></div>
        ${a.needsYou ? '<button data-fix>Review</button>' : '<span class="chip good">ok</span>'}
      </div>`).join('')}`;
  main.querySelectorAll('[data-fix]').forEach(b => b.onclick = () => { VB.stage = 'sync'; if (VB.sync) VB.sync.step = 'plan'; renderVariantB(root); });
}
