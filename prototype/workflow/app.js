/* THROWAWAY PROTOTYPE — router, floating switcher, shared plan/progress renderers.
   The Sync Plan *content* is dictated by closed decision "Define safe
   reconciliation and sync semantics" — so all variants render the same plan
   document; they differ in how you arrive at it and where it lives. */

const PROTOTYPE = true; // gate for the switcher — this artifact never ships

const VARIANTS = {
  A: { name: 'Library Browser', render: (r) => renderVariantA(r) },
  B: { name: 'Guided Flow',     render: (r) => renderVariantB(r) },
  C: { name: 'Pack Home',       render: (r) => renderVariantC(r) },
};
const ORDER = ['A', 'B', 'C'];

function currentVariant() {
  const v = new URLSearchParams(location.search).get('variant');
  return ORDER.includes(v) ? v : 'A';
}
function setVariant(v) {
  const q = new URLSearchParams(location.search);
  q.set('variant', v);
  history.replaceState(null, '', '?' + q.toString());
  renderApp();
}
function renderApp() {
  const v = currentVariant();
  const root = document.getElementById('root');
  root.innerHTML = '';
  VARIANTS[v].render(root);
  renderSwitcher(v);
}
function renderSwitcher(v) {
  if (!PROTOTYPE) return;
  const host = document.getElementById('switcher-host');
  const i = ORDER.indexOf(v);
  host.innerHTML = `
    <div id="proto-switcher">
      <button id="sw-prev" title="Previous variant (←)">←</button>
      <div class="vlabel"><b>${v}</b> — ${esc(VARIANTS[v].name)}</div>
      <button id="sw-next" title="Next variant (→)">→</button>
    </div>`;
  host.querySelector('#sw-prev').onclick = () => setVariant(ORDER[(i + ORDER.length - 1) % ORDER.length]);
  host.querySelector('#sw-next').onclick = () => setVariant(ORDER[(i + 1) % ORDER.length]);
}
document.addEventListener('keydown', (e) => {
  if (/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName || '') || document.activeElement?.isContentEditable) return;
  const i = ORDER.indexOf(currentVariant());
  if (e.key === 'ArrowLeft') setVariant(ORDER[(i + ORDER.length - 1) % ORDER.length]);
  if (e.key === 'ArrowRight') setVariant(ORDER[(i + 1) % ORDER.length]);
});
window.addEventListener('popstate', renderApp);

/* ---------------- shared Sync Plan document (decision #3 content) ---------------- */

function planDocumentHtml(plan, st) {
  const t = plan.target;
  const cap = t.capacity;
  const usedPct = cap ? Math.round((cap.usedGB / cap.totalGB) * 100) : 0;
  const addPct = cap ? Math.max(1, Math.round((cap.planAddGB / cap.totalGB) * 100)) : 0;
  const markPct = cap ? Math.round(((cap.totalGB - cap.marginGB) / cap.totalGB) * 100) : 0;
  const groups = plan.groups.map(g => {
    const pills = [
      g.adds.length ? `<span class="pill add">+${g.adds.length} add</span>` : '',
      g.retains ? `<span class="pill keep">${g.retains} keep</span>` : '',
      g.removes.length ? `<span class="pill del">−${g.removes.length} remove</span>` : '',
    ].join('');
    const items = [
      ...g.adds.map(f => `<div class="gi"><span class="pill add">add</span><span class="mono">${esc(f)}</span></div>`),
      ...g.removes.map(f => `<div class="gi"><span class="pill del">remove</span><span class="mono">${esc(f)}</span></div>`),
    ].join('');
    return `<div class="plan-group">
      <div class="gh"><b>${esc(platformOf(g.platform).short)}</b><span class="small">${esc(platformOf(g.platform).esde)}/</span><span style="flex:1"></span>${pills}</div>
      ${items || '<div class="gi muted">no changes</div>'}
    </div>`;
  }).join('');

  return `
    <div class="plan-meta">
      <div class="m"><div class="k">Media Target</div>${esc(t.name)}<div class="small">${esc(t.binding)}</div></div>
      <div class="m"><div class="k">Device Profile</div>${esc(t.profile)}<div class="small">built-in, immutable</div></div>
      <div class="m"><div class="k">ROM Pack</div>${esc(plan.pack)} <span class="small">${esc(plan.packRev)}</span></div>
      <div class="m"><div class="k">Evidence</div>${esc(plan.scanFreshness)}<div class="small">${esc(plan.verifyFreshness)}</div></div>
    </div>
    ${cap ? `
    <h3>Capacity — peak uses high-water mark</h3>
    <div class="capbar" style="--u:${usedPct}%;--a:${addPct}%;--m:${markPct}%">
      <div class="used"></div><div class="add"></div><div class="mark" title="safe high-water mark (${cap.marginGB} GB margin)"></div>
    </div>
    <div class="small">${cap.usedGB} GB used of ${cap.totalGB} GB · plan adds ${cap.planAddGB} GB · ${cap.marginGB} GB safety margin — fits without relying on removals</div>
    ` : ''}
    <h3 style="margin-top:16px">Actions by Platform</h3>
    ${groups}
    <div class="plan-group">
      <div class="gh"><b>Other actions</b></div>
      <div class="gi"><span class="pill adopt">adopt</span><span class="mono">${esc(plan.adoption.file)}</span><span class="small">— ${esc(plan.adoption.note)}</span></div>
      ${plan.preservedUnknowns.map(u => `<div class="gi"><span class="pill keep">preserve</span><span class="mono">${esc(u)}</span><span class="small">— unrecognized, left untouched</span></div>`).join('')}
    </div>
    ${!st.conflictResolved ? `
      <div class="banner bad" style="margin-top:14px">
        <b>Blocked — path conflict.</b> <span class="mono">${esc(plan.conflict.path)}</span><br>
        <span class="small">${esc(plan.conflict.note)}</span>
        <div style="margin-top:8px; display:flex; gap:8px">
          <button data-plan-act="drop-set">Drop the affected ROM Set from this plan</button>
          <button data-plan-act="refresh">I moved the file on the device — Refresh</button>
        </div>
      </div>` : `
      <div class="banner good" style="margin-top:14px">Conflict resolved — plan refreshed and revalidated. Snapshot unchanged otherwise.</div>`}
    <h3 style="margin-top:16px">Safety guarantees</h3>
    <ul class="small" style="margin:4px 0 12px 18px">${plan.guarantees.map(g => `<li>${esc(g)}</li>`).join('')}</ul>
    <div class="banner ${st.conflictResolved ? '' : ''}" style="display:flex; gap:10px; align-items:center; ${st.conflictResolved ? 'border-color:var(--line);background:var(--bg3)' : ''}">
      <label class="small" style="display:flex; gap:8px; align-items:center; flex:1">
        <input type="checkbox" data-plan-ack ${st.ack ? 'checked' : ''} ${st.conflictResolved ? '' : 'disabled'}>
        This plan permanently removes <b>${plan.totals.remove}</b> managed artifacts. Removal is permanent — no trash on the target.
      </label>
      <button class="primary" data-plan-approve ${st.ack && st.conflictResolved ? '' : 'disabled'}>
        Approve plan — ${plan.totals.add} adds · ${plan.totals.adopt} adoption · ${plan.totals.remove} permanent removals
      </button>
    </div>`;
}

/* Renders the plan into container; calls hooks.onApprove when approved. */
function renderPlanInto(container, plan, st, hooks) {
  container.innerHTML = planDocumentHtml(plan, st);
  const rerender = () => renderPlanInto(container, plan, st, hooks);
  container.querySelectorAll('[data-plan-act]').forEach(b => b.onclick = () => {
    st.conflictResolved = true;
    rerender();
  });
  const ack = container.querySelector('[data-plan-ack]');
  if (ack) ack.onchange = () => { st.ack = ack.checked; rerender(); };
  const ap = container.querySelector('[data-plan-approve]');
  if (ap) ap.onclick = () => hooks.onApprove();
}

/* ---------------- shared progress (decision #3: phases, per-set states) ---------------- */

function progressHtml(sim) {
  const phaseLabel = { adding: 'Phase 1 of 3 — Adding', verifying: 'Phase 2 of 3 — Verifying', removing: 'Phase 3 of 3 — Removing',
    done: 'Complete', 'cancelling…': 'Cancelling…', indeterminate: 'Indeterminate — device disconnected' }[sim.phase] || sim.phase;
  const pct = Math.round((sim.bytesDone / sim.bytesTotal) * 100);
  const rows = sim.steps.filter(s => s.kind === 'add').map(s => `
    <div class="prog-row">
      <div class="state-ic">${s.set.state === 'verified ✓' ? '✅' : s.set.state === 'queued' ? '⏳' : s.set.state === 'verifying' ? '🔍' : '📦'}</div>
      <div><div class="mono">${esc(s.set.name)}</div><div class="prog-bar"><div style="width:${Math.round(s.set.pct)}%"></div></div></div>
      <div class="small" style="text-align:right">${esc(s.set.state)}</div>
    </div>`).join('');
  const others = sim.steps.filter(s => s.kind !== 'add').map(s => `
    <div class="prog-row"><div class="state-ic">${s.state ? '✅' : '⏳'}</div>
      <div class="mono">${esc(s.name || '')}</div>
      <div class="small" style="text-align:right">${s.kind === 'remove' ? (s.state || 'queued — removals run last') : (s.state || 'queued')}</div></div>`).join('');

  let terminal = '';
  if (sim.phase === 'done') terminal = `<div class="banner good" style="margin-top:14px"><b>Sync complete.</b> Every selected artifact placed and verified, adoption recorded, removals absent, manifests agree. Preserved unknowns remain disclosed.</div>`;
  if (sim.phase === 'indeterminate') terminal = `
    <div class="banner bad" style="margin-top:14px"><b>Device disconnected mid-transfer.</b> Destination state is indeterminate. Completed additions are retained; remaining removals were skipped.<br>
    <span class="small">The application never auto-resumes. Reconnect, refresh, and confirm a new Sync Plan.</span>
    <div style="margin-top:8px"><button data-prog-refresh>Reconnect &amp; refresh target</button></div></div>`;
  if (sim.cancelled && sim.phase !== 'indeterminate') terminal = `
    <div class="banner" style="margin-top:14px"><b>Cancelled.</b> In-flight write finished safely; no removals were started. Completed additions remain. Refresh and create a new plan to continue.</div>`;

  return `
    <div style="display:flex; align-items:center; gap:12px; margin-bottom:8px">
      <h3 style="flex:1; margin:0">${esc(phaseLabel)}</h3>
      ${!sim.done && !sim.disconnected && !sim.cancelled ? '<button data-prog-cancel>Cancel</button>' : ''}
    </div>
    <div class="small">${Math.round(sim.bytesDone)} MB of ${sim.bytesTotal} MB (${pct}%) · verify follows every write · removals run last</div>
    <div class="prog-bar" style="height:8px; margin:8px 0 14px"><div style="width:${pct}%"></div></div>
    ${rows}${others}${terminal}`;
}

function runProgressInto(container, plan, opts) {
  opts = opts || {};
  const sim = makeSyncSim(plan, { disconnectAt: opts.disconnectAt, onTick: paint });
  function paint() {
    container.innerHTML = progressHtml(sim);
    const c = container.querySelector('[data-prog-cancel]');
    if (c) c.onclick = () => sim.cancel();
    const r = container.querySelector('[data-prog-refresh]');
    if (r) r.onclick = () => { sim.stop(); if (opts.onRefresh) opts.onRefresh(); };
    if (sim.done && opts.onDone) opts.onDone(sim);
  }
  paint();
  sim.start();
  return sim;
}

/* boot */
renderApp();
