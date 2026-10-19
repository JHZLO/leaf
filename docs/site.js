// subwy landing page.
// The first screen is a sample project drawn as a metro map: one folder, one line; its files are its
// stations. Scrolling rides the map stop by stop, then the page carries on to the window, the
// timetable and the tickets. Values that follow every scroll frame go to CSS variables; everything
// else re-renders only when it changes.
(() => {
  const $ = (id) => document.getElementById(id);

  // The root is the loop; the other lines run through their folder's station and on past the edges.
  const LINES = [
    { key: 'metro-notes', color: '#2E6BE6', loop: true, nodes: [[940, 630, 'metro-notes'], [940, 500], [1060, 380], [1200, 380, 'docs'], [1340, 380], [1400, 440, 'tests'], [1460, 500], [1460, 630, 'README.md'], [1460, 760], [1340, 880], [1060, 880], [1000, 820, 'src'], [940, 760]] },
    { key: 'metro-notes/docs', color: '#12A150', nodes: [[1200, -300], [1200, 170, 'guide'], [1200, 280, 'changelog.md'], [1200, 380, 'docs'], [1200, 1300]] },
    { key: 'metro-notes/docs/guide', color: '#F0306E', nodes: [[-400, 300], [580, 300], [710, 170], [1040, 170, 'getting-started.md'], [1200, 170, 'guide'], [1900, 170]] },
    { key: 'metro-notes/src', color: '#FF7A00', nodes: [[1000, 820, 'src'], [900, 920], [720, 920, 'lines.rs'], [540, 920, 'main.rs'], [-400, 920]] },
    { key: 'metro-notes/tests', color: '#7B5BE6', nodes: [[1400, 440, 'tests'], [1480, 360], [1480, 280, 'lines.rs'], [1480, -300]] }
  ];
  const PLACE = {
    'metro-notes': 'r', 'metro-notes/docs': 'se', 'metro-notes/tests': 'l', 'metro-notes/README.md': 'l', 'metro-notes/src': 'r',
    'metro-notes/docs/guide': 'ne', 'metro-notes/docs/changelog.md': 'l', 'metro-notes/docs/guide/getting-started.md': 'b',
    'metro-notes/src/lines.rs': 'a', 'metro-notes/src/main.rs': 'a', 'metro-notes/tests/lines.rs': 'l'
  };
  const SIDES = {
    a: 'left:50%;bottom:50px;transform:translateX(-50%);', b: 'left:50%;top:50px;transform:translateX(-50%);',
    l: 'right:52px;top:50%;transform:translateY(-50%);', r: 'left:52px;top:50%;transform:translateY(-50%);',
    ne: 'left:44px;bottom:44px;', se: 'left:44px;top:44px;'
  };
  const TOUR = ['metro-notes/docs/guide/getting-started.md', 'metro-notes/src/main.rs', 'metro-notes/README.md', 'metro-notes/tests/lines.rs', 'metro-notes/docs/changelog.md'];
  // Each stop of the ride is a station on the map and a part of the window (crop: x, y, width, as fractions).
  const STEPS = [
    { dest: 'metro-notes/docs/guide/getting-started.md', title: 'Walk the tree. The way is drawn.', text: 'Arrow keys move the selection, and the way to it is drawn as a metro line, a new color at every folder. It glides as you move. Space passes through: the file shows and you stay in the tree.', keys: [[['↑', '↓'], 'Move'], [['←', '→'], 'Fold'], [['Space'], 'Pass through']], crop: [0, 0.03, 0.42], part: 'the tree' },
    { dest: 'metro-notes/src/main.rs', title: 'Jump to a name. Copy its path.', text: 'Type letters or press Cmd+F to jump to a name. Copy the absolute path, or the one relative to the file’s own folder; from the editor the line comes along, as in src/main.rs:12.', keys: [[['⌘', 'F'], 'Find'], [['⌘', '⇧', 'C'], 'Absolute'], [['⌘', '⌥', '⇧', 'C'], 'Relative']], crop: [0.24, 0.035, 0.42], part: 'the tabs and the route path' },
    { dest: 'metro-notes/README.md', title: 'Markdown, side by side.', text: 'Read Markdown and code with syntax highlighting, the raw source included. The preview sits beside the editor or alone; drag the line between them.', keys: [[['⌘', '⇧', 'V'], 'Preview beside']], crop: [0.6, 0.1, 0.4], part: 'the Markdown preview' },
    { dest: 'metro-notes/tests/lines.rs', title: 'Your shell, one keystroke away.', text: 'The terminal opens in the folder. Hide it and the shell keeps running; open another tab whenever you need one.', keys: [[['⌥', 'F12'], 'Terminal'], [['⌘', 'T'], 'New tab']], crop: [0, 0.6, 0.4], part: 'the terminal' }
  ];
  const NAV_COLORS = ['#2E6BE6', '#12A150', '#F0306E', '#FF7A00'];
  const SPOTS = [
    { name: 'Tree', color: '#12A150', box: [0.4, 4.4, 23.6, 59] },
    { name: 'Tabs', color: '#FF7A00', box: [24.4, 4.4, 38, 9] },
    { name: 'Preview', color: '#F0306E', box: [62.4, 9.5, 37.2, 53.5] },
    { name: 'Terminal', color: '#7B5BE6', box: [0.4, 63.4, 99.2, 32.6] }
  ];
  const TIMES = [['LAUNCH TO FIRST WINDOW', '0.36', 's'], ['MEMORY, 5 S AFTER LAUNCH', '60', 'MB'], ['DOWNLOAD (DMG)', '10.3', 'MB'], ['ON DISK', '33', 'MB'], ['CPU WHILE IDLE', '<1', '%']];
  const TICKETS = [
    { id: 'build', kind: 'SINGLE', title: 'Build the app', color: '#12A150', ink: '#0E7E3F', lines: [
      ['git clone https://github.com/JHZLO/subwy', ''], ['cd subwy', ''], ['scripts/bundle.sh', '         # target/bundle/subwy.app'], ['cp -R target/bundle/subwy.app /Applications/', '']] },
    { id: 'cli', kind: 'RETURN', title: 'The subwy command', color: '#FF7A00', ink: '#B4560A', lines: [
      ['mkdir -p ~/.local/bin', ''], ['ln -s "$PWD/scripts/subwy" ~/.local/bin/subwy', ''], ['subwy ~/notes', '     # the folder, in its window']] }
  ];
  const GLYPHS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-';

  const now0 = Date.now();
  const S = {
    now: now0, at: now0, tick: 1, phase: 0, cam: { s: 0.9, x: 0, y: 0 },
    sel: TOUR[0], hover: null, auto: true,
    navStop: 0, navReached: 0, past: false, spot: null, theme: 'light',
    timeAt: null, watch: 0, copied: {}, seen: {}
  };

  // Map model
  const nodePath = (line, name) => (name === line.key.split('/').pop() ? line.key : line.key + '/' + name);
  const STATIONS = [];
  LINES.forEach((line) => line.nodes.forEach((n) => {
    if (!n[2]) return;
    const path = nodePath(line, n[2]);
    if (path !== line.key || path === 'metro-notes') {
      STATIONS.push({ path, name: n[2], x: n[0], y: n[1], color: line.color, folder: LINES.some((l) => l.key === path) });
    }
  }));

  // The stretch of one line between two of its stations; on the loop, the shorter way round.
  function stretch(line, from, to) {
    const nodes = line.nodes;
    const i = nodes.findIndex((n) => n[2] === from);
    const j = nodes.findIndex((n) => n[2] === to);
    if (!line.loop) return i <= j ? nodes.slice(i, j + 1) : nodes.slice(j, i + 1).reverse();
    const walk = (dir) => {
      const out = [nodes[i]];
      for (let k = i; k !== j;) { k = (k + dir + nodes.length) % nodes.length; out.push(nodes[k]); }
      return out;
    };
    const length = (pts) => pts.reduce((sum, p, k) => (k ? sum + Math.hypot(p[0] - pts[k - 1][0], p[1] - pts[k - 1][1]) : 0), 0);
    const a = walk(1);
    const b = walk(-1);
    return length(a) <= length(b) ? a : b;
  }

  // The way from the root to a path, one piece per folder line it rides.
  function route(path) {
    if (!path) return [];
    const parts = path.split('/');
    const segs = [];
    for (let k = 1; k < parts.length; k++) {
      const line = LINES.find((l) => l.key === parts.slice(0, k).join('/'));
      const pts = stretch(line, parts[k - 1], parts[k]);
      segs.push({ color: line.color, name: parts[k - 1], pts, d: 'M' + pts.map((p) => p[0] + ' ' + p[1]).join(' L') });
    }
    return segs;
  }

  // DOM writes only when the value changes, so animations restart only when they should.
  const setCss = (el, css) => { if (el && el.__css !== css) { el.style.cssText = css; el.__css = css; } };
  const setCls = (el, cls) => { if (el && el.__cls !== cls) { el.setAttribute('class', cls); el.__cls = cls; } };
  const setText = (el, text) => { if (el && el.__text !== text) { el.textContent = text; el.__text = text; } };
  const setHtml = (el, html) => { if (el && el.__html !== html) { el.innerHTML = html; el.__html = html; } };
  const setAttr = (el, name, value) => { if (el && el['__' + name] !== value) { el.setAttribute(name, value); el['__' + name] = value; } };
  const esc = (t) => String(t).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  const textOn = (c) => (c === '#FF7A00' ? '#1F2023' : '#FFFFFF');
  const set = (patch) => { Object.assign(S, patch); render(); };

  // Built once
  const stage = $('stage');
  const stationEls = STATIONS.map((x) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'stn';
    b.innerHTML = '<span class="halo" aria-hidden="true"></span><span class="mark" aria-hidden="true"></span><span class="lbl"></span>';
    b.querySelector('.lbl').textContent = x.name;
    b.addEventListener('click', () => { if (S.phase === 0) set({ sel: x.path, hover: null, at: Date.now(), tick: S.tick + 1, auto: false }); });
    const enter = () => { if (S.phase === 0 && S.hover !== x.path) set({ hover: x.path }); };
    const leave = () => { if (S.hover) set({ hover: null }); };
    b.addEventListener('mouseenter', enter);
    b.addEventListener('focus', enter);
    b.addEventListener('mouseleave', leave);
    b.addEventListener('blur', leave);
    stage.appendChild(b);
    return { x, b, halo: b.children[0], mark: b.children[1], lbl: b.children[2] };
  });
  const tileEls = Array.from({ length: 18 }, () => { const t = document.createElement('span'); $('tiles').appendChild(t); return t; });
  const navStops = Array.from(document.querySelectorAll('.navstop'));

  const spotsEl = $('spots');
  const spotLegs = SPOTS.slice(1).map((x, i) => {
    const leg = document.createElement('span');
    leg.className = 'spot-leg';
    leg.setAttribute('aria-hidden', 'true');
    leg.style.left = 'calc(22px + (100% - 44px) * ' + (i / 3) + ')';
    leg.style.background = x.color;
    spotsEl.appendChild(leg);
    return leg;
  });
  const spotEls = SPOTS.map((x, i) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'spot';
    b.style.left = 'calc(22px + (100% - 44px) * ' + (i / 3) + ')';
    b.innerHTML = '<span class="spot-dot" aria-hidden="true"></span><span class="spot-name"></span>';
    b.querySelector('.spot-name').textContent = x.name;
    const pick = () => { if (S.spot !== x.name) set({ spot: x.name }); };
    b.addEventListener('click', pick);
    b.addEventListener('mouseenter', pick);
    b.addEventListener('focus', pick);
    spotsEl.appendChild(b);
    return { x, b, dot: b.children[0] };
  });
  $('segLight').addEventListener('click', () => set({ theme: 'light' }));
  $('segDark').addEventListener('click', () => set({ theme: 'dark' }));

  const timesEl = $('times');
  const timeEls = TIMES.map((t) => {
    const row = document.createElement('div');
    row.className = 'time-row';
    row.innerHTML = '<span class="time-label"></span><span class="time-dots" aria-hidden="true"></span><span class="time-value"></span><span class="time-unit"></span>';
    row.children[0].textContent = t[0];
    row.children[3].textContent = t[2];
    timesEl.appendChild(row);
    return row.children[2];
  });

  const ticketEls = TICKETS.map((t) => {
    const card = document.createElement('div');
    card.className = 'ticket';
    card.innerHTML =
      '<span class="ticket-band" aria-hidden="true" style="background:' + t.color + '"></span>' +
      '<div class="ticket-head"><div><span class="ticket-kind" style="color:' + t.ink + '">' + t.kind + '</span><span class="ticket-title">' + esc(t.title) + '</span></div>' +
      '<button class="btn btn-dark" type="button">Copy</button></div>' +
      '<div class="ticket-cut" aria-hidden="true"></div><span class="notch l" aria-hidden="true"></span><span class="notch r" aria-hidden="true"></span>' +
      '<div class="ticket-cmds">' + t.lines.map((l) => '<div><span style="color:' + t.color + '">$ </span>' + esc(l[0]) + '<span class="note">' + esc(l[1]) + '</span></div>').join('') + '</div>';
    const button = card.querySelector('button');
    button.addEventListener('click', () => {
      const done = (v) => set({ copied: Object.assign({}, S.copied, { [t.id]: v }) });
      try { navigator.clipboard.writeText(t.lines.map((l) => l[0]).join('\n')).then(() => done('ok'), () => done('fail')); } catch (e) { done('fail'); }
    });
    $('ticketList').appendChild(card);
    return { t, button };
  });

  const anims = {};
  function depart() {
    cancelAnimationFrame(anims.watch);
    const t0 = performance.now();
    const step = (t) => {
      const p = Math.min(1, (t - t0) / 360);
      set({ watch: p });
      if (p < 1) anims.watch = requestAnimationFrame(step);
    };
    set({ watch: 0 });
    anims.watch = requestAnimationFrame(step);
  }
  $('depart').addEventListener('click', depart);

  // Frame the whole map for the first screen, then each stop's route beside its card.
  function camera(phase, W, H, card) {
    const wide = W > 1100;
    if (phase === 0) {
      const s = wide && W >= H ? Math.min(W / 1600, H / 1000) : Math.max(W / 1600, H / 1000);
      return { s, x: (W - 1600 * s) / 2, y: (H - 1000 * s) / 2 };
    }
    const pts = [];
    route(STEPS[phase - 1].dest).forEach((seg) => seg.pts.forEach((p) => pts.push(p)));
    const pad = 130;
    const xs = pts.map((p) => p[0]);
    const ys = pts.map((p) => p[1]);
    const bx = Math.min(...xs) - pad;
    const by = Math.min(...ys) - pad;
    const bw = Math.max(...xs) + pad - bx;
    const bh = Math.max(...ys) + pad - by;
    const area = wide
      ? { x: card.right + 24, y: 76, w: Math.max(200, W - card.right - 48), h: H - 116 }
      : { x: 16, y: 72, w: W - 32, h: Math.max(160, card.top - 88) };
    const s = Math.min(area.w / bw, area.h / bh, 1.3);
    return { s, x: area.x + (area.w - bw * s) / 2 - bx * s, y: area.y + (area.h - bh * s) / 2 - by * s };
  }

  // Where the reader is: the ride's stop, the camera, the header's train, what has come into view.
  function measure() {
    const vp = $('viewport');
    const ride = $('map');
    const W = vp.clientWidth;
    const H = vp.clientHeight;
    const r = ride.getBoundingClientRect();
    const p = Math.min(1, Math.max(0, -r.top / Math.max(1, r.height - H)));
    const phase = Math.min(4, Math.round(p * 4));
    const next = {};
    if (phase !== S.phase) Object.assign(next, { phase, tick: S.tick + 1, at: Date.now(), hover: null });

    const card = $(phase === 0 ? 'heroCard' : 'stepCard').getBoundingClientRect();
    const box = vp.getBoundingClientRect();
    const cam = camera(phase, W, H, { right: card.right - box.left, top: card.top - box.top });
    if (Math.abs(cam.s - S.cam.s) > 0.001 || Math.abs(cam.x - S.cam.x) > 0.5 || Math.abs(cam.y - S.cam.y) > 0.5) next.cam = cam;

    const tops = ['map', 'ride', 'window', 'timetable', 'tickets'].map((id) => $(id).getBoundingClientRect().top);
    const probe = H * 0.4;
    let pos = 0;
    tops.forEach((t, i) => {
      if (t > probe) return;
      const after = tops[i + 1];
      pos = after === undefined ? i : i + Math.min(1, (probe - t) / Math.max(1, after - t));
    });
    $('navline').style.setProperty('--p', pos.toFixed(4));
    if (Math.round(pos) !== S.navStop) next.navStop = Math.round(pos);
    if (Math.floor(pos + 0.01) !== S.navReached) next.navReached = Math.floor(pos + 0.01);
    const past = r.top < -H * 0.5;
    if (past !== S.past) next.past = past;

    const q = Math.min(1, Math.max(0, (H - $('window').getBoundingClientRect().top - 120) / (H * 0.8)));
    $('frame').style.setProperty('--q', q.toFixed(4));

    const seen = Object.assign({}, S.seen);
    let more = false;
    ['window', 'timetable', 'tickets', 'end'].forEach((id) => {
      if (!seen[id] && $(id).getBoundingClientRect().top < H * 0.85) { seen[id] = true; more = true; }
    });
    if (more) next.seen = seen;
    if (more && seen.timetable && !S.seen.timetable) next.timeAt = Date.now();
    if (Object.keys(next).length) set(next);
    if (next.timeAt) depart();
  }

  function render() {
    const riding = S.phase > 0;
    const step = STEPS[Math.max(0, S.phase - 1)];
    const sel = riding ? step.dest : S.sel;
    const segs = route(sel);
    const ghost = !riding && S.hover && S.hover !== sel ? route(S.hover) : [];
    const parts = sel.split('/');
    const onRoute = {};
    parts.forEach((p, k) => { onRoute[parts.slice(0, k + 1).join('/')] = true; });
    const dest = STATIONS.find((x) => x.path === sel);
    const anim = S.tick % 2 ? 'A' : 'B';

    // Header
    setCls($('header'), 'header' + (S.past ? ' past' : ''));
    navStops.forEach((a, i) => {
      setCls(a, 'navstop' + (i === S.navStop ? ' here' : ''));
      setCss(a.firstElementChild, 'border-color:' + (S.navReached >= i ? NAV_COLORS[Math.min(i, 3)] : '#D6D3CC'));
    });

    // Map
    setCss(stage, 'transform:translate(' + S.cam.x + 'px,' + S.cam.y + 'px) scale(' + S.cam.s + ')');
    setCss($('base'), 'opacity:' + (riding ? 0.3 : 0.45));
    [0, 1, 2].forEach((i) => {
      const seg = segs[i];
      const r = $('r' + i);
      const f = $('f' + i);
      setAttr(r, 'd', seg ? seg.d : 'M0 0');
      setAttr(r, 'stroke', seg ? seg.color : 'transparent');
      setCls(r, seg ? 'draw' + anim : '');
      setAttr(f, 'd', seg ? seg.d : 'M0 0');
      setCss(f, seg ? 'opacity:.95;animation-delay:' + (0.45 * segs.length) + 's' : 'opacity:0');
      setAttr($('g' + i), 'd', ghost[i] ? ghost[i].d : 'M0 0');
    });
    stationEls.forEach(({ x, b, halo, mark, lbl }) => {
      const isDest = x.path === sel;
      const lit = !!onRoute[x.path];
      const hot = x.path === S.hover;
      let m;
      if (isDest) m = 'width:42px;height:42px;border-radius:50%;background:' + x.color + ';box-shadow:0 0 0 7px #FBFAF8,0 0 0 10px ' + x.color + ';';
      else if (x.folder) m = 'width:40px;height:40px;border-radius:50%;background:#FFFFFF;border:8px solid #1F2023;';
      else m = 'width:30px;height:30px;border-radius:50%;background:#FFFFFF;border:7px solid ' + x.color + ';';
      setCss(b, 'left:' + (x.x - 28) + 'px;top:' + (x.y - 28) + 'px;z-index:' + (isDest ? 3 : 2) + ';opacity:' + (riding && !lit ? 0.35 : 1));
      setCss(mark, m + 'transform:scale(' + (hot ? 1.15 : 1) + ')');
      setCss(halo, 'background:' + x.color);
      setCls(halo, 'halo' + (isDest ? ' pulse' : ''));
      setCss(lbl, SIDES[PLACE[x.path] || 'b'] + (isDest
        ? 'padding:3px 12px;border-radius:10px;font-weight:800;background:' + x.color + ';color:' + textOn(x.color) + ';'
        : 'padding:0 6px;border-radius:8px;background:rgba(251,250,248,.85);font-weight:' + (lit || hot ? 800 : 600) + ';color:' + (lit || hot ? '#1F2023' : '#5E5F64') + ';'));
    });

    // Hero card: the departure board
    setCls($('heroCard'), 'card' + (riding ? ' off' : ''));
    setCls($('cue'), 'cue' + (riding ? ' off' : ''));
    const d = new Date(S.now);
    const pad = (n) => ('0' + n).slice(-2);
    setText($('clock'), pad(d.getHours()) + ':' + pad(d.getMinutes()) + ':' + pad(d.getSeconds()));
    const name = dest.name.toUpperCase();
    const lead = Math.floor((18 - name.length) / 2);
    // The name sits in the middle slots; only its letters spin, the slots around it stay blank.
    tileEls.forEach((el, i) => {
      const c = i - lead;
      if (c < 0 || c >= name.length) { setCls(el, 'tile empty'); setText(el, ''); return; }
      const done = S.now >= S.at + 260 + 55 * c;
      setCls(el, 'tile' + (done ? '' : ' spin'));
      setText(el, done ? name[c] : GLYPHS[(Math.floor(S.now / 45) * 7 + i * 13) % GLYPHS.length]);
    });
    const stops = parts.length - 1;
    const stopsText = stops + (stops === 1 ? ' STOP' : ' STOPS') + ' FROM METRO-NOTES';
    const last = segs[segs.length - 1];
    setText($('heroSr'), 'Next stop: ' + dest.name + ', ' + stopsText);
    setText($('stopsText'), stopsText);
    setText($('lineName'), (last ? last.name : 'metro-notes').toUpperCase() + ' LINE');
    setCss($('lineDot'), 'background:' + (last ? last.color : '#2E6BE6'));
    setCss($('trail'), 'grid-template-columns:repeat(' + parts.length + ',minmax(0,1fr))');
    setHtml($('trail'), parts.map((p, k) => {
      const isLast = k === parts.length - 1;
      const into = segs[k - 1];
      const out = segs[k];
      const bar = (seg, side, delay) => (seg ? '<span class="trail-bar grow' + anim + '" style="' + side + 'background:' + seg.color + ';animation-delay:' + delay + 's"></span>' : '');
      const dot = isLast
        ? 'width:20px;height:20px;border-radius:50%;background:' + dest.color + ';box-shadow:0 0 0 4px #1F2023,0 0 0 7px ' + dest.color + '55'
        : 'width:18px;height:18px;border-radius:50%;background:#1F2023;border:4.5px solid #FBFAF8';
      return '<div class="trail-stop"><div class="trail-rail" aria-hidden="true">' +
        bar(into, 'left:0;right:50%;', (k - 1) * 0.45 + 0.22) + bar(out, 'left:50%;right:0;', k * 0.45) +
        '<span style="position:relative;' + dot + '"></span></div><span class="trail-name' + (isLast ? ' last' : '') + '">' + esc(p) + '</span></div>';
    }).join(''));

    // Step card: one stop of the ride
    setCls($('stepCard'), 'card card-step' + (riding ? '' : ' off'));
    setCls($('stepInner'), 'card-box in' + anim);
    const no = Math.max(1, S.phase);
    setText($('stepChip'), String(no));
    setCss($('stepChip'), 'background:' + dest.color + ';color:' + textOn(dest.color));
    setText($('stepCount'), 'STOP ' + no + ' OF 4');
    setText($('stepDest'), dest.name);
    setCss($('stepDest'), 'background:' + dest.color + '1F;box-shadow:inset 0 0 0 1.5px ' + dest.color);
    setText($('stepTitle'), step.title);
    setText($('stepText'), step.text);
    setHtml($('stepKeys'), step.keys.map((k) => '<span class="key">' + k[0].map((c) => '<span class="kc">' + esc(c) + '</span>').join('') + '<span class="key-what">' + esc(k[1]) + '</span></span>').join(''));
    const cw = 100 / step.crop[2];
    setCss($('crop'), 'width:' + cw + '%;left:' + (-step.crop[0] * cw) + '%;top:' + (-step.crop[1] * cw) + '%');
    setAttr($('crop'), 'alt', 'subwy, zoomed to ' + step.part);

    // Window
    setCls($('riseWindow'), 'rise' + (S.seen.window ? '' : ' wait'));
    setCls($('riseTime'), 'rise' + (S.seen.timetable ? '' : ' wait'));
    setCls($('riseTickets'), 'rise' + (S.seen.tickets ? '' : ' wait'));
    const spot = SPOTS.find((x) => x.name === S.spot);
    spotLegs.forEach((leg) => { leg.style.opacity = spot ? 0.35 : 1; });
    spotEls.forEach(({ x, b, dot }) => {
      const on = S.spot === x.name;
      setCls(b, 'spot' + (on ? ' on' : ''));
      setAttr(b, 'aria-pressed', String(on));
      setCss(dot, on ? 'width:28px;height:28px;margin-top:-4px;border:0;background:' + x.color + ';box-shadow:0 0 0 5px #FBFAF8,0 0 0 8px ' + x.color : '');
    });
    setCls($('segLight'), 'seg' + (S.theme === 'light' ? ' on' : ''));
    setCls($('segDark'), 'seg' + (S.theme === 'dark' ? ' on' : ''));
    setAttr($('segLight'), 'aria-pressed', String(S.theme === 'light'));
    setAttr($('segDark'), 'aria-pressed', String(S.theme === 'dark'));
    setCss($('shotLight'), 'opacity:' + (S.theme === 'light' ? 1 : 0));
    setCss($('shotDark'), 'opacity:' + (S.theme === 'dark' ? 1 : 0));
    setCss($('spotBox'), spot
      ? 'left:' + spot.box[0] + '%;top:' + spot.box[1] + '%;width:' + spot.box[2] + '%;height:' + spot.box[3] + '%;box-shadow:0 0 0 3px ' + spot.color + ',0 0 0 4000px rgba(31,32,35,.42)'
      : '');

    // Timetable
    TIMES.forEach((t, i) => {
      let value = ' --';
      if (S.timeAt !== null) {
        const at = S.timeAt + i * 140;
        value = (t[1] + '    ').slice(0, 4).split('').map((ch, k) => (S.now >= at + 300 + 60 * k ? ch : GLYPHS[(Math.floor(S.now / 45) * 7 + k * 13 + (i + 4) * 5) % GLYPHS.length])).join('').replace(/\s+$/, '');
      }
      setText(timeEls[i], value);
    });
    setText($('watch'), (0.36 * S.watch).toFixed(2));
    setCss($('runLine'), 'width:' + (S.watch * 100) + '%');
    setCls($('arrive'), 'watch-end' + (S.watch >= 1 ? ' in' : ''));

    // Tickets and the end of the line
    ticketEls.forEach(({ t, button }) => {
      const v = S.copied[t.id];
      setText(button, v === 'ok' ? 'Copied' : v === 'fail' ? 'Select to copy' : 'Copy');
    });
    setCls($('lay'), 'lay' + (S.seen.end ? ' go' : ''));
  }

  let frame = 0;
  const onScroll = () => {
    if (frame) return;
    frame = requestAnimationFrame(() => { frame = 0; measure(); });
  };
  window.addEventListener('scroll', onScroll, { passive: true });
  window.addEventListener('resize', onScroll);

  // The clock ticks every second; the boards flip in 45 ms steps while they settle.
  setInterval(() => {
    const now = Date.now();
    const moving = now < S.at + 1500 || (S.timeAt && now < S.timeAt + 1600);
    if (moving || Math.floor(now / 1000) !== Math.floor(S.now / 1000)) set({ now });
  }, 45);
  // Until someone picks a station, the first screen tours a few of them.
  setInterval(() => {
    if (!S.auto || S.phase !== 0) return;
    set({ sel: TOUR[(TOUR.indexOf(S.sel) + 1) % TOUR.length], at: Date.now(), tick: S.tick + 1 });
  }, 4600);

  render();
  measure();
  window.addEventListener('load', measure);
})();
