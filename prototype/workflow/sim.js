/* THROWAWAY PROTOTYPE — shared sync-run simulator (tick engine only, no layout).
   Simulates #3 semantics: additions → verify → removals last; cooperative cancel;
   a scripted mid-transfer disconnect that ends indeterminate pending refresh. */

function makeSyncSim(plan, opts) {
  opts = opts || {};
  const sets = [];
  plan.groups.forEach(g => g.adds.forEach(f => sets.push({ name: f, platform: g.platform, state: 'queued', pct: 0 })));
  // merge adopt into flow as its own step
  const steps = sets.map(s => ({ kind: 'add', set: s }));
  steps.push({ kind: 'adopt', name: plan.adoption.file });
  plan.groups.forEach(g => g.removes.forEach(r => steps.push({ kind: 'remove', name: r })));

  const sim = {
    steps,
    idx: -1,
    phase: 'adding',           // adding → verifying → removing → done | cancelled | disconnected
    done: false,
    cancelled: false,
    disconnected: false,
    bytesDone: 0,
    bytesTotal: 2200,          // MB, fake
    verifyPct: 0,
    timer: null,
    onTick: opts.onTick || function () {},
  };

  function tick() {
    if (sim.done) return;

    // scripted disconnect at ~60% of additions
    if (opts.disconnectAt != null && sim.idx === opts.disconnectAt && !sim._pulled) {
      sim._pulled = true;
      sim.disconnected = true;
      sim.phase = 'indeterminate';
      stop();
      sim.onTick(sim);
      return;
    }
    if (sim.cancelled) {
      stop();
      sim.onTick(sim);
      return;
    }

    // advance current step
    const cur = sim.steps[sim.idx];
    if (cur && cur.kind === 'add' && cur.set.pct < 100) {
      cur.set.pct = Math.min(100, cur.set.pct + 7 + Math.random() * 9);
      cur.set.state = cur.set.pct >= 100 ? 'verifying' : 'copying';
      sim.bytesDone = Math.min(sim.bytesTotal, sim.bytesDone + 60 + Math.random() * 80);
      if (cur.set.state === 'verifying') { cur.set.pct = 100; }
      sim.onTick(sim);
      return;
    }
    if (cur && cur.kind === 'add' && cur.set.state === 'verifying') {
      cur.set.state = 'verified ✓';
      sim.onTick(sim);
      return;
    }

    // next step
    sim.idx++;
    const next = sim.steps[sim.idx];
    if (!next) { sim.phase = 'done'; sim.done = true; stop(); sim.onTick(sim); return; }
    if (next.kind === 'add') { sim.phase = 'adding'; next.set.state = 'copying'; next.set.pct = 1; }
    if (next.kind === 'adopt') { sim.phase = 'verifying'; next.state = 'adopted ✓'; }
    if (next.kind === 'remove') {
      sim.phase = 'removing';
      // removals happen only after all adds verified
      next.state = 'removed';
    }
    sim.onTick(sim);
  }

  function stop() { if (sim.timer) { clearInterval(sim.timer); sim.timer = null; } }

  sim.start = function () { stop(); sim.timer = setInterval(tick, 420); };
  sim.cancel = function () { sim.cancelled = true; sim.phase = 'cancelling…'; sim.onTick(sim); };
  sim.stop = stop;
  return sim;
}
