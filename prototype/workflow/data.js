/* THROWAWAY PROTOTYPE — fake domain data, realistic density.
   Mirrors the closed decisions: Game/Release/ROM Set model (#5),
   Media Target/Device Profile behavior (#6), sync semantics (#3),
   metadata policy (#9), first-release Platforms (#14). */

const PLATFORMS = [
  { key: 'nes',     short: 'NES',     name: 'Nintendo Entertainment System', esde: 'nes' },
  { key: 'snes',    short: 'SNES',    name: 'Super Nintendo',                esde: 'snes' },
  { key: 'gb',      short: 'GB',      name: 'Game Boy',                      esde: 'gb' },
  { key: 'gba',     short: 'GBA',     name: 'Game Boy Advance',              esde: 'gba' },
  { key: 'n64',     short: 'N64',     name: 'Nintendo 64',                   esde: 'n64' },
  { key: 'genesis', short: 'GEN',     name: 'Sega Genesis / Mega Drive',     esde: 'megadrive' },
  { key: 'psx',     short: 'PSX',     name: 'Sony PlayStation',              esde: 'psx' },
];

/* art = hue for the CSS-generated box-art tile. kind: '' | 'provisional' | 'hack' */
const GAMES = [
  { id: 'g-smb3',   title: 'Super Mario Bros. 3',        platform: 'nes', hue: 8,   sets: [
    { id: 's-smb3-us', label: 'USA (Rev A)', sizeMB: 0.4, files: ['Super Mario Bros. 3 (USA) (Rev A).nes'], complete: true }]},
  { id: 'g-loz',    title: 'The Legend of Zelda',        platform: 'nes', hue: 120, sets: [
    { id: 's-loz-us', label: 'USA', sizeMB: 0.1, files: ['Legend of Zelda, The (USA).nes'], complete: true }]},
  { id: 'g-mm2',    title: 'Mega Man 2',                 platform: 'nes', hue: 210, sets: [
    { id: 's-mm2-us', label: 'USA', sizeMB: 0.3, files: ['Mega Man 2 (USA).nes'], complete: true }]},
  { id: 'g-ct',     title: 'Chrono Trigger',             platform: 'snes', hue: 265, sets: [
    { id: 's-ct-us', label: 'USA', sizeMB: 4, files: ['Chrono Trigger (USA).sfc'], complete: true },
    { id: 's-ct-pg', label: "USA — 'Prophet's Guile' (hack, derived from USA)", sizeMB: 4, files: ["Chrono Trigger - Prophet's Guile (USA) (hack).sfc"], complete: true, hack: true }]},
  { id: 'g-sm',     title: 'Super Metroid',              platform: 'snes', hue: 20,  sets: [
    { id: 's-sm-us', label: 'USA/Japan', sizeMB: 3, files: ['Super Metroid (USA) (En,Ja).sfc'], complete: true }]},
  { id: 'g-eb',     title: 'EarthBound',                 platform: 'snes', hue: 45,  sets: [
    { id: 's-eb-us', label: 'USA', sizeMB: 3, files: ['EarthBound (USA).sfc'], complete: true }]},
  { id: 'g-alttp',  title: 'Zelda: A Link to the Past',  platform: 'snes', hue: 95,  sets: [
    { id: 's-alttp-us', label: 'USA', sizeMB: 1, files: ['Legend of Zelda, The - A Link to the Past (USA).sfc'], complete: true }]},
  { id: 'g-tetris', title: 'Tetris',                     platform: 'gb',  hue: 190, sets: [
    { id: 's-tetris-w', label: 'World (Rev A)', sizeMB: 0.1, files: ['Tetris (World) (Rev A).gb'], complete: true }]},
  { id: 'g-pkmn',   title: 'Pokémon Red Version',        platform: 'gb',  hue: 355, sets: [
    { id: 's-pkmn-us', label: 'USA/Europe', sizeMB: 1, files: ['Pokemon - Red Version (USA, Europe).gb'], complete: true }]},
  { id: 'g-la',     title: "Link's Awakening",           platform: 'gb',  hue: 140, sets: [
    { id: 's-la-us', label: 'USA/Europe (Rev 2)', sizeMB: 0.5, files: ["Legend of Zelda, The - Link's Awakening (USA, Europe) (Rev 2).gb"], complete: true }]},
  { id: 'g-mf',     title: 'Metroid Fusion',             platform: 'gba', hue: 175, sets: [
    { id: 's-mf-us', label: 'USA/Europe', sizeMB: 8, files: ['Metroid Fusion (USA, Europe).gba'], complete: true }]},
  { id: 'g-aw',     title: 'Advance Wars',               platform: 'gba', hue: 60,  sets: [
    { id: 's-aw-us', label: 'USA', sizeMB: 8, files: ['Advance Wars (USA).gba'], complete: true }]},
  { id: 'g-aos',    title: 'Castlevania: Aria of Sorrow',platform: 'gba', hue: 285, sets: [
    { id: 's-aos-us', label: 'USA', sizeMB: 8, files: ['Castlevania - Aria of Sorrow (USA).gba'], complete: true }]},
  { id: 'g-sm64',   title: 'Super Mario 64',             platform: 'n64', hue: 0,   sets: [
    { id: 's-sm64-us', label: 'USA', sizeMB: 8, files: ['Super Mario 64 (USA).z64'], complete: true }]},
  { id: 'g-oot',    title: 'Zelda: Ocarina of Time',     platform: 'n64', hue: 80,  sets: [
    { id: 's-oot-us', label: 'USA (Rev 2)', sizeMB: 32, files: ['Legend of Zelda, The - Ocarina of Time (USA) (Rev 2).z64'], complete: true }]},
  { id: 'g-sonic2', title: 'Sonic the Hedgehog 2',       platform: 'genesis', hue: 225, sets: [
    { id: 's-sonic2-w', label: 'World', sizeMB: 1, files: ['Sonic the Hedgehog 2 (World).md'], complete: true }]},
  { id: 'g-sor2',   title: 'Streets of Rage 2',          platform: 'genesis', hue: 30, sets: [
    { id: 's-sor2-us', label: 'USA', sizeMB: 2, files: ['Streets of Rage 2 (USA).md'], complete: true }]},
  { id: 'g-ff8',    title: 'Final Fantasy VIII',         platform: 'psx', hue: 240, sets: [
    { id: 's-ff8-us', label: 'USA — 3 discs (CHD + M3U)', sizeMB: 1410,
      files: ['Final Fantasy VIII (USA) (Disc 1).chd', 'Final Fantasy VIII (USA) (Disc 2).chd', 'Final Fantasy VIII (USA) (Disc 3).chd', 'Final Fantasy VIII (USA).m3u'], complete: true }]},
  { id: 'g-sotn',   title: 'Castlevania: Symphony of the Night', platform: 'psx', hue: 300, sets: [
    { id: 's-sotn-us', label: 'USA (CHD)', sizeMB: 390, files: ['Castlevania - Symphony of the Night (USA).chd'], complete: true }]},
  { id: 'g-sui2',   title: 'Suikoden II',                platform: 'psx', hue: 200, sets: [
    { id: 's-sui2-us', label: 'USA — 2 discs (1 of 2 present)', sizeMB: 430,
      files: ['Suikoden II (USA) (Disc 1).chd'], complete: false, expected: 2 }]},
  { id: 'g-myst',   title: 'Mystery Cart Dump',          platform: 'gba', hue: 150, provisional: true, sets: [
    { id: 's-myst-x', label: 'Unidentified dump (user-titled)', sizeMB: 16, files: ['cart_dump_07.gba'], complete: true }]},
  { id: 'g-dw4h',   title: 'dragon-warrior-4-hack',      platform: 'nes', hue: 250, provisional: true, sets: [
    { id: 's-dw4h-x', label: 'Unidentified (user-titled)', sizeMB: 0.4, files: ['dragon-warrior-4-hack.nes'], complete: true }]},
];

const SOURCES = [
  { id: 'src1', path: '~/ROMs/No-Intro',       status: 'indexed',  files: 16411, matched: 1587, note: '' },
  { id: 'src2', path: '~/ROMs/PSX (CHD)',      status: 'indexed',  files: 74,    matched: 19,   note: '3 unrecognized files preserved' },
  { id: 'src3', path: '~/Downloads/roms-misc', status: 'indexing', files: null,  matched: null, note: 'Indexing… 41% (scanning archive members)' },
];

const REVIEW_QUEUE = [
  { id: 'rq1', file: 'dragon-warrior-4-hack.nes', suggestion: 'Dragon Warrior IV (NES) — filename evidence only', kind: 'weak-evidence' },
  { id: 'rq2', file: 'Suikoden II (USA) (Disc 1).chd', suggestion: 'Suikoden II (USA) — partial set (1 of 2 discs)', kind: 'partial-set' },
];

const PACKS = [
  { id: 'p-odin', name: 'Odin Essentials', setIds: ['s-smb3-us','s-loz-us','s-mm2-us','s-ct-us','s-sm-us','s-alttp-us','s-tetris-w','s-pkmn-us','s-mf-us','s-aw-us','s-sm64-us','s-sonic2-w','s-sotn-us','s-ct-pg'] },
  { id: 'gbtrip', name: 'Road Trip — GB/GBA', setIds: ['s-tetris-w','s-pkmn-us','s-la-us','s-mf-us','s-aw-us','s-aos-us'] },
];

const TARGETS = [
  { id: 't-sd',   name: 'Odin 3 SD Card (512 GB)', profile: 'ES-DE on Android v3.1', binding: 'Card reader — E:\\ (filesystem)', online: true,
    capacity: { totalGB: 512, usedGB: 431, planAddGB: 2.1, marginGB: 8 },
    managed: 34, marker: 'rommgr.target 7f3a…c9', scan: 'current' },
  { id: 't-int',  name: 'Odin 3 — Internal storage', profile: 'ES-DE on Android v3.1', binding: 'MTP — AYN Odin 3 (USB)', online: false,
    capacity: null, managed: 12, marker: 'rommgr.target 1b08…44', scan: 'stale — disconnected 3 days ago' },
];

/* The scripted Sync Plan: Odin Essentials → Odin 3 SD Card.
   Honors #3: immutable snapshot, grouped actions, adoption, permanent removals,
   preserved unknowns, an occupant conflict that blocks, capacity high-water mark. */
function buildPlan() {
  return {
    pack: 'Odin Essentials', packRev: 'rev 14',
    target: TARGETS[0],
    scanFreshness: 'scanned 2 min ago', verifyFreshness: 'hashes current',
    groups: [
      { platform: 'nes', adds: ['Super Mario Bros. 3 (USA) (Rev A).nes', 'Mega Man 2 (USA).nes'], retains: 1, removes: [] },
      { platform: 'snes', adds: ["Chrono Trigger - Prophet's Guile (USA) (hack).sfc"], retains: 3, removes: ['Chrono Trigger (USA) (Rev 1).sfc — superseded by managed Rev A'] },
      { platform: 'gb', adds: [], retains: 2, removes: [] },
      { platform: 'gba', adds: ['Advance Wars (USA).gba'], retains: 1, removes: [] },
      { platform: 'n64', adds: ['Super Mario 64 (USA).z64'], retains: 0, removes: [] },
      { platform: 'genesis', adds: [], retains: 1, removes: ['Sonic the Hedgehog (USA).md — deselected'] },
      { platform: 'psx', adds: ['Castlevania - Symphony of the Night (USA).chd'], retains: 0, removes: ['Legend of Dragoon, The (USA) (Disc 1).chd — deselected'] },
    ],
    adoption: { file: 'megadrive/Sonic the Hedgehog 2 (World).md', note: 'unrecognized file byte-equals selected artifact → adopt into management' },
    preservedUnknowns: ['psx/homebrew-demo.chd', 'roms.txt', '.DS_Store'],
    conflict: { path: 'nes/Legend of Zelda, The (USA).nes', note: 'unrecognized occupant at a required canonical path — bytes differ. Resolve before this plan can run.' },
    totals: { add: 6, addBytes: '2.1 GB', retain: 8, remove: 3, adopt: 1, artifacts: 18 },
    guarantees: ['Additions verified by read-back hash before any removal', 'Unknown content is preserved', 'No automatic resume after interruption'],
  };
}

/* Capacity-blocked scenario: same pack → Internal storage if it were online. */
const CAPACITY_BLOCK = { neededGB: 2.1, freeGB: 0.9, note: 'Additions and staging must fit without relying on planned removals.' };

/* Which ROM Sets live on each Media Target (from its last inventory/scan).
   t-sd matches the plan's 8 retains + the adoption candidate's set.
   Stale inventories are marked — presence from them is "last known". */
const TARGET_INVENTORY = {
  't-sd':  { sets: ['s-loz-us','s-ct-us','s-sm-us','s-alttp-us','s-tetris-w','s-pkmn-us','s-mf-us','s-sonic2-w'],
             scan: 'last scan 2 min ago', stale: false, short: 'SD' },
  't-int': { sets: ['s-tetris-w','s-pkmn-us','s-la-us','s-mf-us','s-aw-us','s-aos-us'],
             scan: 'last scan 3 days ago', stale: true, short: 'INT' },
};
function setsOnTarget(game, targetId) {
  const inv = TARGET_INVENTORY[targetId];
  return inv ? game.sets.filter(s => inv.sets.includes(s.id)) : [];
}
function gameOnTarget(game, targetId) { return setsOnTarget(game, targetId).length > 0; }

/* Lookup helpers */
const byId = {};
GAMES.forEach(g => g.sets.forEach(s => { byId[s.id] = { game: g, set: s }; }));
function setInfo(id) { return byId[id]; }
function platformOf(key) { return PLATFORMS.find(p => p.key === key); }
function packSets(pack) {
  return pack.setIds.map(id => byId[id]).filter(Boolean);
}
function packSizeMB(pack) {
  return packSets(pack).reduce((n, x) => n + x.set.sizeMB, 0);
}
function fmtSize(mb) {
  return mb >= 1024 ? (mb / 1024).toFixed(1) + ' GB' : mb >= 1 ? Math.round(mb) + ' MB' : Math.round(mb * 1024) + ' KB';
}
function esc(s) { const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }
/* opts: { lens: targetId|null } — renders a device-presence dot when the game
   is on the lens target (amber when that inventory is stale). */
function artTile(game, cls, opts) {
  opts = opts || {};
  const label = game.provisional ? '?' : game.title.split(/[:\s]/).filter(Boolean).slice(0, 2).map(w => w[0]).join('');
  const plat = platformOf(game.platform);
  let dot = '';
  if (opts.lens && gameOnTarget(game, opts.lens)) {
    const inv = TARGET_INVENTORY[opts.lens];
    dot = `<i class="dev-dot ${inv.stale ? 'stale' : ''}" title="On ${esc(TARGETS.find(t => t.id === opts.lens).name)} (${esc(inv.scan)})"></i>`;
  }
  return `<div class="art ${cls || ''}" style="--hue:${game.hue}"><span>${esc(label)}</span>${game.provisional ? '<i class="prov-dot" title="Provisional — locally identified"></i>' : ''}${dot}<i class="plat-tag">${esc(plat.short)}</i></div>`;
}
