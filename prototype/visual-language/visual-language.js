/* THROWAWAY PROTOTYPE - visual systems over the settled Library Browser. */

const VISUALS = {
  A: 'Cartridge Index',
  B: 'Signal Deck',
  C: 'After Hours',
};

function applyVisual(key) {
  document.documentElement.dataset.visual = key;
  document.body.classList.toggle('scale-200', currentScale() === '200');
}

function enableKeyboardGames(root) {
  root.querySelectorAll('[data-game]').forEach((game) => {
    game.tabIndex = 0;
    game.setAttribute('role', 'button');
    game.onkeydown = (event) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        game.click();
      }
    };
  });
}

Object.keys(VISUALS).forEach((key) => {
  VARIANTS[key].name = VISUALS[key];
  VARIANTS[key].render = (root) => {
    applyVisual(key);
    renderVariantA(root);
    enableKeyboardGames(root);
  };
});

function currentScale() {
  return new URLSearchParams(location.search).get('scale') === '200' ? '200' : '100';
}

function replaceQuery(updates) {
  const query = new URLSearchParams(location.search);
  Object.entries(updates).forEach(([key, value]) => query.set(key, value));
  history.replaceState(null, '', '?' + query.toString());
}

function setScale(scale) {
  replaceQuery({ scale });
  renderApp();
}

function showLibrary() {
  replaceQuery({ surface: 'library' });
  VA.wizard = null;
  VA.view = { kind: 'platform', key: 'all' };
  VA.selectedGame = 'g-ct';
  renderApp();
}

function showRiskState() {
  replaceQuery({ surface: 'risk' });
  VA.wizard = {
    step: 3,
    packId: 'p-odin',
    targetId: 't-sd',
    plan: buildPlan(),
    planSt: { ack: false, conflictResolved: false },
    scenario: 'normal',
  };
  renderApp();
}

renderSwitcher = function renderVisualSwitcher(key) {
  const host = document.getElementById('switcher-host');
  const index = ORDER.indexOf(key);
  const scale = currentScale();
  const surface = new URLSearchParams(location.search).get('surface') === 'risk' ? 'risk' : 'library';
  host.innerHTML = `
    <div id="proto-switcher" aria-label="Prototype controls">
      <button id="sw-prev" title="Previous visual system (Left arrow)" aria-label="Previous visual system">&larr;</button>
      <div class="vlabel"><b>${key}</b> - ${esc(VISUALS[key])}</div>
      <button id="sw-next" title="Next visual system (Right arrow)" aria-label="Next visual system">&rarr;</button>
      <span class="switcher-rule"></span>
      <button class="sw-text ${surface === 'library' ? 'on' : ''}" id="sw-library">Library</button>
      <button class="sw-text ${surface === 'risk' ? 'on' : ''}" id="sw-risk">Risk state</button>
      <span class="switcher-rule"></span>
      <button class="sw-text ${scale === '100' ? 'on' : ''}" id="sw-100">100%</button>
      <button class="sw-text ${scale === '200' ? 'on' : ''}" id="sw-200">200%</button>
    </div>`;
  host.querySelector('#sw-prev').onclick = () => setVariant(ORDER[(index + ORDER.length - 1) % ORDER.length]);
  host.querySelector('#sw-next').onclick = () => setVariant(ORDER[(index + 1) % ORDER.length]);
  host.querySelector('#sw-library').onclick = showLibrary;
  host.querySelector('#sw-risk').onclick = showRiskState;
  host.querySelector('#sw-100').onclick = () => setScale('100');
  host.querySelector('#sw-200').onclick = () => setScale('200');
};

VA.selectedGame = 'g-ct';
if (new URLSearchParams(location.search).get('surface') === 'risk') showRiskState();
else renderApp();
