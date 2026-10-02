// gluck showreel — 15s deterministic canvas motion piece.
// Every frame is a pure function of t, so the timeline can be scrubbed freely.
(() => {
  'use strict';

  const W = 1280, H = 720, DUR = 15, FPS = 24;
  const C = {
    bg: '#0a0e13', bg2: '#0f151c', panel: '#121920', panel2: '#17202a', line: '#24303c',
    fg: '#e7ecf1', dim: '#7c8897', faint: '#3a4552',
    mint: '#7dd3a0', blue: '#8ab4f8', amber: '#f3c26b', rose: '#f28b9b', violet: '#b79cf2',
  };
  const MONO = '"SF Mono", ui-monospace, Menlo, Consolas, "Liberation Mono", monospace';
  const SANS = '"Pretendard", "Apple SD Gothic Neo", -apple-system, system-ui, "Noto Sans KR", "Malgun Gothic", sans-serif';

  // ---------------------------------------------------------------- math
  const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
  const seg = (t, a, b) => clamp((t - a) / (b - a));
  const lerp = (a, b, p) => a + (b - a) * p;
  const E = {
    linear: p => p,
    outCubic: p => 1 - Math.pow(1 - p, 3),
    inCubic: p => p * p * p,
    inOutCubic: p => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2),
    outExpo: p => (p >= 1 ? 1 : 1 - Math.pow(2, -10 * p)),
    inExpo: p => (p <= 0 ? 0 : Math.pow(2, 10 * p - 10)),
    inOutExpo: p => (p <= 0 ? 0 : p >= 1 ? 1 : p < 0.5 ? Math.pow(2, 20 * p - 10) / 2 : (2 - Math.pow(2, -20 * p + 10)) / 2),
    outBack: p => { const c1 = 1.70158, c3 = c1 + 1; return 1 + c3 * Math.pow(p - 1, 3) + c1 * Math.pow(p - 1, 2); },
    spring: p => (p >= 1 ? 1 : 1 - Math.exp(-6.5 * p) * Math.cos(13 * p)),
  };
  const tw = (t, a, b, ease = E.outExpo) => ease(seg(t, a, b));
  // fade in over [a, a+fi], fade out over [b-fo, b]
  const life = (t, a, b, fi = 0.2, fo = 0.2) => Math.min(seg(t, a, a + fi), 1 - seg(t, b - fo, b));

  function rng(seed) {
    return () => {
      seed = (seed + 0x6d2b79f5) | 0;
      let r = Math.imul(seed ^ (seed >>> 15), 1 | seed);
      r = (r + Math.imul(r ^ (r >>> 7), 61 | r)) ^ r;
      return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
    };
  }

  function hexA(hex, a) {
    const n = parseInt(hex.slice(1), 16);
    return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
  }

  // ---------------------------------------------------------------- data (from git history)
  const DAYS = [
    ['05·21', 35], ['05·22', 71], ['05·23', 32], ['05·24', 43], ['05·25', 14], ['05·26', 44],
    ['05·27', 31], ['06·06', 1], ['06·08', 7], ['06·17', 9], ['08·18', 12], ['08·19', 1],
    ['08·22', 14], ['09·04', 2], ['09·05', 4], ['10·01', 27],
  ];
  const MONTH_COLOR = { '05': C.mint, '06': C.blue, '08': C.amber, '09': C.violet, '10': C.rose };
  const TAGS = [];
  [
    [1, ['0.1.0', '0.1.1', '0.2.0', '0.2.1', '0.2.2', '0.3.0', '0.3.1', '0.4.0', '0.5.0', '0.5.1', '0.5.2', '0.5.3', '0.5.4']],
    [2, ['0.5.5', '0.5.6', '0.5.7', '0.5.8', '0.5.9', '0.5.10', '0.5.11', '0.5.12', '0.5.13']],
    [3, ['0.7.4']], [4, ['0.8.1']], [5, ['0.9.0']], [6, ['0.9.3', '0.9.5']], [8, ['0.9.6', '0.10.0']],
    [9, ['0.10.1']], [10, ['0.11.0']], [12, ['0.12.0', '0.12.1']], [13, ['0.13.0']],
    [15, ['0.13.1', '0.13.2', '0.14.0', '0.15.0']],
  ].forEach(([d, vs]) => vs.forEach(v => TAGS.push({ v: 'v' + v, d })));

  const MRR = [0.330, 0.616, 0.743, 0.750, 0.318, 0.563, 0.610, 0.607, 0.611, 0.607,
    0.463, 0.450, 0.479, 0.487, 0.493, 0.494, 0.517, 0.517, 0.498, 0.508];
  const CATS = [['ident', 0.815], ['natural', 0.531], ['korean', 0.556], ['typo', 0.522], ['para', 0.344], ['commit', 0.278]];
  const LANGS = ['Rust', 'Go', 'C', 'TypeScript', 'TSX', 'JavaScript', 'Swift', 'Zig', 'Bash', 'JSON', 'YAML', 'Markdown', 'Asm', 'ld', 'Lisette'];

  const COMMITS = [
    ['af466a8', 'HANDOFF 갱신: v0.15.0 릴리즈 완료'],
    ['d67b8d7', 'Bump version to 0.15.0'],
    ['2d28377', 'README와 사이트에 오타 교정 설명 추가'],
    ['38e90dd', '검색 결과 제목 camelCase 분해 문제 수정'],
    ['5b25fb9', 'glc에 -V/--version 옵션 추가'],
    ['fdb0841', 'negative 쿼리에 no strong match 표시'],
    ['d632bf9', 'typo 쿼리 교정 추가: 편집거리 1'],
    ['75c15dd', '.glcignore에 기본 제외 목록 추가'],
    ['0b0e66f', 'Bump version to 0.14.0'],
    ['9d3fa22', 'glc ignore 명령 추가'],
    ['f6dc23e', 'negative 판별 신호 probe 예제 추가'],
    ['bf01a88', '.glcignore로 인덱싱 제외 지원'],
  ];

  const TREE = [
    ['▾ src/', 0, C.blue], ['▸ git/', 1, C.blue], ['▸ highlight/', 1, C.blue], ['▾ search/', 1, C.blue],
    ['bm25.rs', 2], ['embedding.rs', 2], ['indexer.rs', 2], ['rrf.rs', 2, null, true], ['typo.rs', 2],
    ['vector.rs', 2], ['▸ ui/', 1, C.blue], ['app.rs', 1], ['main.rs', 1],
  ];
  const TREE_SEL = 7;

  const CODE_NEW = [
    'pub fn rrf_fuse_weighted(',
    '    bm25: &[(u64, f32)],',
    '    vec: &[(u64, f32)],',
    '    k: f32,',
    '    limit: usize,',
    '    w_bm25: f32,',
    '    w_vec: f32,',
    ') -> Vec<(u64, f32)> {',
    '    // id -> (fused score, best rank)',
    '    let mut scores = HashMap::new();',
    '    for (list, w) in [(bm25, w_bm25), (vec, w_vec)] {',
    '        for (rank, (id, _)) in list.iter().enumerate() {',
    '            let e = scores.entry(*id).or_insert((0.0, usize::MAX));',
    '            e.0 += w / (k + rank as f32 + 1.0);',
    '            e.1 = e.1.min(rank);',
  ];
  const CODE_OLD = CODE_NEW.slice();
  CODE_OLD[5] = '';
  CODE_OLD[6] = '';
  CODE_OLD[10] = '    for list in [bm25, vec] {';
  CODE_OLD[13] = '            e.0 += 1.0 / (k + rank as f32 + 1.0);';
  const CHANGED = new Set([5, 6, 10, 13]);

  const RESULTS = [
    ['commit', 'afafd87', 'BM25 스키마를 구조화 필드로 재설계 (INDEX_VERSION 4)', 0.94],
    ['commit', '877a57b', 'Bump INDEX_VERSION to 6 for path_terms schema', 0.81],
    ['symbol', 'indexer.rs', 'build_index · meta.toml 버전 확인 후 재빌드', 0.72],
    ['file', 'search/mod.rs', 'SearchEngine::open — 스키마 호환성 검사', 0.58],
    ['commit', '38e90dd', '검색 결과 제목 camelCase 분해 문제 수정 (v11)', 0.47],
  ];
  const QUERY = '인덱스 스키마 바뀌면 재빌드';
  const BIGRAMS = ['인덱', '덱스', '스키', '키마', '재빌', '빌드'];

  // ---------------------------------------------------------------- layout constants
  const AXIS_Y = 560, COL_X0 = 150, COL_X1 = 1130;
  const colX = d => lerp(COL_X0, COL_X1, d / (DAYS.length - 1));
  const WIN = { x: 140, y: 104, w: 1000, h: 532 };
  const LP = { x: 158, y: 156, w: 360, h: 444 };   // left panel
  const RP = { x: 534, y: 156, w: 588, h: 444 };   // right panel
  const ROW_H = 34;
  const rowY = r => LP.y + 10 + r * ROW_H;

  // ---------------------------------------------------------------- precomputed dots
  const DOTS = [];
  (() => {
    const R = rng(7);
    let gi = 0;
    DAYS.forEach(([label, n], d) => {
      const color = MONTH_COLOR[label.slice(0, 2)];
      for (let k = 0; k < n; k++) {
        const col = k % 3, row = Math.floor(k / 3);
        DOTS.push({
          d, gi: gi++, color,
          x: colX(d) + (col - 1) * 8,
          y: AXIS_Y - 12 - row * 8,
          jit: R() * 0.12,
          tr: Math.floor(R() * COMMITS.length),
          tx: LP.x + 92 + R() * 230,
        });
      }
    });
  })();

  // landing sparks for logo letters
  const SPARKS = [];
  (() => {
    const R = rng(42);
    for (let i = 0; i < 5; i++) {
      const list = [];
      for (let k = 0; k < 12; k++) {
        const a = -Math.PI * (0.08 + R() * 0.84);
        const v = 140 + R() * 260;
        list.push({ vx: Math.cos(a) * v, vy: Math.sin(a) * v, r: 1 + R() * 2.2, c: R() < 0.5 ? C.mint : C.blue });
      }
      SPARKS.push(list);
    }
  })();

  // ---------------------------------------------------------------- drawing helpers
  let ctx;
  function rr(x, y, w, h, r) {
    r = Math.max(0, Math.min(r, w / 2, h / 2));
    ctx.beginPath();
    ctx.moveTo(x + r, y);
    ctx.arcTo(x + w, y, x + w, y + h, r);
    ctx.arcTo(x + w, y + h, x, y + h, r);
    ctx.arcTo(x, y + h, x, y, r);
    ctx.arcTo(x, y, x + w, y, r);
    ctx.closePath();
  }
  function txt(s, x, y, font, color, align = 'left', base = 'alphabetic') {
    ctx.font = font;
    ctx.fillStyle = color;
    ctx.textAlign = align;
    ctx.textBaseline = base;
    ctx.fillText(s, x, y);
  }
  const mono = (size, weight = 400) => `${weight} ${size}px ${MONO}`;
  const sans = (size, weight = 400) => `${weight} ${size}px ${SANS}`;

  // tiny Rust highlighter for the View/Diff panes
  const KW = /^(pub|fn|let|mut|for|in|as|use|return|if|else|impl|struct)$/;
  const TY = /^(u64|f32|usize|Vec|HashMap|Self|Option)$/;
  function drawCode(line, x, y, size) {
    ctx.font = mono(size);
    ctx.textBaseline = 'alphabetic';
    ctx.textAlign = 'left';
    const ci = line.indexOf('//');
    const body = ci >= 0 ? line.slice(0, ci) : line;
    const parts = body.split(/(\w+|\s+|[^\w\s])/).filter(Boolean);
    let cx = x;
    for (const p of parts) {
      let col = C.fg;
      if (KW.test(p)) col = C.violet;
      else if (TY.test(p)) col = C.amber;
      else if (/^\d/.test(p)) col = C.rose;
      else if (/^(rrf_fuse_weighted|iter|enumerate|entry|or_insert|min|new)$/.test(p)) col = C.blue;
      else if (/^[^\w\s]$/.test(p)) col = '#9aa6b4';
      ctx.fillStyle = col;
      ctx.fillText(p, cx, y);
      cx += ctx.measureText(p).width;
    }
    if (ci >= 0) {
      ctx.fillStyle = C.dim;
      ctx.fillText(line.slice(ci), cx, y);
    }
  }

  function keycap(t, label, at) {
    const a = life(t, at - 0.06, at + 0.6, 0.08, 0.22);
    if (a <= 0) return;
    const pop = E.outBack(seg(t, at - 0.06, at + 0.16));
    const press = seg(t, at, at + 0.06) * (1 - seg(t, at + 0.1, at + 0.2));
    ctx.save();
    ctx.globalAlpha = a;
    ctx.font = mono(18, 600);
    const w = Math.max(46, ctx.measureText(label).width + 30);
    const x = W / 2 - w / 2, y = 662 + press * 3;
    ctx.translate(W / 2, y + 22);
    ctx.scale(0.7 + 0.3 * pop, 0.7 + 0.3 * pop);
    ctx.translate(-W / 2, -(y + 22));
    ctx.shadowColor = hexA(C.mint, 0.5);
    ctx.shadowBlur = 24 * (1 - press);
    rr(x, y + 4 - press * 3, w, 40, 8);
    ctx.fillStyle = '#05080b';
    ctx.fill();
    ctx.shadowBlur = 0;
    rr(x, y, w, 40, 8);
    ctx.fillStyle = C.panel2;
    ctx.fill();
    ctx.strokeStyle = hexA(C.mint, 0.85);
    ctx.lineWidth = 1.5;
    ctx.stroke();
    txt(label, W / 2, y + 26, mono(18, 600), C.mint, 'center');
    ctx.restore();
  }

  // ---------------------------------------------------------------- background
  let grain = null;
  function makeGrain() {
    const g = document.createElement('canvas');
    g.width = g.height = 256;
    const gx = g.getContext('2d');
    const img = gx.createImageData(256, 256);
    const R = rng(3);
    for (let i = 0; i < img.data.length; i += 4) {
      const v = R() * 255;
      img.data[i] = img.data[i + 1] = img.data[i + 2] = v;
      img.data[i + 3] = 255;
    }
    gx.putImageData(img, 0, 0);
    return g;
  }

  const GLOWS = [
    [0, C.mint, 640, 330], [2.6, C.blue, 900, 420], [5.4, C.mint, 380, 300], [8.6, C.violet, 640, 360],
    [11.7, C.amber, 820, 300], [13.7, C.mint, 640, 330], [15, C.mint, 640, 330],
  ];
  function background(t) {
    ctx.fillStyle = C.bg;
    ctx.fillRect(0, 0, W, H);

    let i = 0;
    while (i < GLOWS.length - 2 && t >= GLOWS[i + 1][0]) i++;
    const [t0, c0, x0, y0] = GLOWS[i], [t1, c1, x1, y1] = GLOWS[i + 1];
    const p = E.inOutCubic(seg(t, t0, t1 - (t1 - t0) * 0.4));
    const gx = lerp(x0, x1, p), gy = lerp(y0, y1, p);
    for (const [c, a] of [[c0, 1 - p], [c1, p]]) {
      if (a <= 0.01) continue;
      const g = ctx.createRadialGradient(gx, gy, 0, gx, gy, 620);
      g.addColorStop(0, hexA(c, 0.16 * a));
      g.addColorStop(1, hexA(c, 0));
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, W, H);
    }

    // drifting dot grid
    const off = (t * 9) % 32;
    ctx.fillStyle = hexA('#ffffff', 0.045);
    for (let y = -32 + off; y < H; y += 32)
      for (let x = -32 + off * 0.5; x < W; x += 32) ctx.fillRect(x, y, 1.4, 1.4);
  }

  function finish(t) {
    const v = ctx.createRadialGradient(W / 2, H / 2, H * 0.35, W / 2, H / 2, H * 0.95);
    v.addColorStop(0, 'rgba(0,0,0,0)');
    v.addColorStop(1, 'rgba(0,0,0,0.55)');
    ctx.fillStyle = v;
    ctx.fillRect(0, 0, W, H);
    if (!grain) grain = makeGrain();
    const f = Math.floor(t * FPS);
    ctx.save();
    ctx.globalAlpha = 0.045;
    ctx.globalCompositeOperation = 'overlay';
    const ox = (f * 73) % 256, oy = (f * 151) % 256;
    for (let y = -oy; y < H; y += 256) for (let x = -ox; x < W; x += 256) ctx.drawImage(grain, x, y);
    ctx.restore();
  }

  // ---------------------------------------------------------------- HUD
  const CHAPTERS = [[0, '01', 'boot'], [2.6, '02', 'history'], [5.3, '03', 'pick · view · diff'],
    [8.45, '04', 'semantic search'], [11.68, '05', 'measured'], [13.7, '06', 'glc']];
  function hud(t) {
    const a = seg(t, 0.9, 1.4) * 0.75;
    if (a <= 0) return;
    ctx.save();
    ctx.globalAlpha = a;
    let ci = 0;
    CHAPTERS.forEach((c, i) => { if (t >= c[0]) ci = i; });
    const [ct, num, name] = CHAPTERS[ci];
    const k = E.outExpo(seg(t, ct, ct + 0.5));
    ctx.save();
    ctx.beginPath();
    ctx.rect(30, 676, 400, 30);
    ctx.clip();
    txt(num, 40, 698 + (1 - k) * 22, mono(12, 600), C.mint);
    txt(name, 66, 698 + (1 - k) * 22, mono(12), C.dim);
    ctx.restore();
    const f = Math.min(Math.floor(t * FPS), DUR * FPS);
    const tc = `00:00:${String(Math.floor(f / FPS)).padStart(2, '0')}:${String(f % FPS).padStart(2, '0')}`;
    txt(tc, W - 40, 698, mono(12), C.dim, 'right');
    txt('SHOWREEL 2026', W - 40, 40, mono(11, 600), C.faint, 'right');
    // chapter progress ticks
    for (let i = 0; i < CHAPTERS.length; i++) {
      ctx.fillStyle = i <= ci ? hexA(C.mint, 0.8) : hexA('#ffffff', 0.15);
      ctx.fillRect(W - 40 - (CHAPTERS.length - i) * 16, 674, 12, 2);
    }
    ctx.restore();
  }

  // ---------------------------------------------------------------- logo (persists across scenes)
  const logoLayer = document.createElement('canvas');
  const LOGO = 'gluck';
  function logoState(t) {
    const corner = { x: 92, y: 48, s: 0.2 };
    const center = { x: 640, y: 330, s: 1 };
    const outro = { x: 640, y: 272, s: 0.92 };
    if (t < 2.3) return center;
    if (t < 13.45) {
      const p = E.inOutExpo(seg(t, 2.3, 2.85));
      return { x: lerp(center.x, corner.x, p), y: lerp(center.y, corner.y, p), s: lerp(center.s, corner.s, p) };
    }
    const p = E.inOutExpo(seg(t, 13.45, 14.0));
    return { x: lerp(corner.x, outro.x, p), y: lerp(corner.y, outro.y, p), s: lerp(corner.s, outro.s, p) };
  }

  function drawLogo(t, scaleX) {
    if (t < 1.0) return;
    const st = logoState(t);
    const lc = logoLayer.getContext('2d');
    if (logoLayer.width !== ctx.canvas.width || logoLayer.height !== ctx.canvas.height) {
      logoLayer.width = ctx.canvas.width;
      logoLayer.height = ctx.canvas.height;
    }
    lc.setTransform(1, 0, 0, 1, 0, 0);
    lc.clearRect(0, 0, logoLayer.width, logoLayer.height);
    lc.setTransform(scaleX, 0, 0, scaleX, 0, 0);
    lc.translate(st.x, st.y);
    lc.scale(st.s, st.s);
    const size = 150;
    lc.font = `700 ${size}px ${MONO}`;
    lc.textAlign = 'center';
    lc.textBaseline = 'alphabetic';
    const adv = lc.measureText('g').width * 0.98;
    for (let i = 0; i < 5; i++) {
      const s0 = 1.0 + i * 0.07;
      const p = seg(t, s0, s0 + 0.7);
      if (p <= 0) continue;
      const sp = E.spring(p);
      lc.save();
      lc.translate((i - 2) * adv, (1 - sp) * -120 + size * 0.34);
      lc.rotate((1 - sp) * (i % 2 ? 0.25 : -0.25));
      lc.globalAlpha = clamp(p * 5);
      const ox = (i - 2) * adv;
      const grad = lc.createLinearGradient(-adv * 2.5 - ox, 0, adv * 2.5 - ox, 0);
      grad.addColorStop(0, C.mint);
      grad.addColorStop(1, C.blue);
      lc.fillStyle = grad;
      lc.fillText(LOGO[i], 0, 0);
      lc.restore();
    }
    // shine sweep
    const sh = seg(t, 14.25, 14.9);
    if (sh > 0 && sh < 1) {
      const sx = lerp(-adv * 4, adv * 4, E.inOutCubic(sh));
      const g = lc.createLinearGradient(sx - 60, -size, sx + 60, size * 0.4);
      g.addColorStop(0, 'rgba(255,255,255,0)');
      g.addColorStop(0.5, 'rgba(255,255,255,0.75)');
      g.addColorStop(1, 'rgba(255,255,255,0)');
      lc.globalCompositeOperation = 'source-atop';
      lc.fillStyle = g;
      lc.fillRect(-adv * 3, -size, adv * 6, size * 1.6);
      lc.globalCompositeOperation = 'source-over';
    }
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    if (t < 2.4 || t > 13.8) {
      ctx.shadowColor = hexA(C.mint, 0.35);
      ctx.shadowBlur = 40 * scaleX;
    }
    ctx.drawImage(logoLayer, 0, 0);
    ctx.restore();

    // landing sparks
    for (let i = 0; i < 5; i++) {
      const land = 1.0 + i * 0.07 + 0.09;
      const age = t - land;
      if (age < 0 || age > 0.7) continue;
      const bx = 640 + (i - 2) * adv, by = 330 + size * 0.34;
      ctx.save();
      ctx.globalAlpha = 1 - age / 0.7;
      for (const s of SPARKS[i]) {
        ctx.fillStyle = s.c;
        ctx.beginPath();
        ctx.arc(bx + s.vx * age, by + s.vy * age + 420 * age * age, s.r * (1 - age / 0.7), 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.restore();
    }
  }

  // ---------------------------------------------------------------- S1 boot
  function sceneBoot(t) {
    // prompt typing + CRT collapse
    if (t < 1.15) {
      const s = '$ glc';
      const n = Math.min(s.length, Math.floor(seg(t, 0.1, 0.62) * s.length + (t > 0.1 ? 1 : 0)));
      const squash = E.inExpo(seg(t, 0.84, 1.0));
      ctx.save();
      ctx.translate(640, 360);
      ctx.scale(1 + squash * 0.6, 1 - squash * 0.97);
      ctx.font = mono(40, 500);
      const cw = ctx.measureText('M').width;
      const x0 = -(s.length * cw) / 2;
      for (let i = 0; i < n; i++) txt(s[i], x0 + i * cw, 14, mono(40, 500), i === 0 ? C.mint : C.fg);
      if (Math.floor(t * 2.6) % 2 === 0 || t > 0.6) {
        ctx.fillStyle = C.mint;
        ctx.fillRect(x0 + n * cw + 4, -18, cw * 0.62, 40);
      }
      ctx.restore();
      // horizontal flash line
      const fl = seg(t, 0.95, 1.15);
      if (fl > 0) {
        const w = 900 * (1 - E.inCubic(fl));
        ctx.fillStyle = hexA('#ffffff', 0.9 * (1 - fl));
        ctx.fillRect(640 - w / 2, 359, w, 2);
      }
    }
    // radial flash
    const fa = life(t, 0.98, 1.5, 0.04, 0.45);
    if (fa > 0) {
      const g = ctx.createRadialGradient(640, 360, 0, 640, 360, 500);
      g.addColorStop(0, hexA(C.mint, 0.35 * fa));
      g.addColorStop(1, hexA(C.mint, 0));
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, W, H);
    }

    // tagline: acronym initials first, then the rest of each word types out
    const out = 1 - E.inCubic(seg(t, 2.2, 2.45));
    if (t > 1.4 && out > 0) {
      const words = ['git', 'log,', 'unfolds', 'code', 'into', 'knowledge.'];
      const hot = [true, true, true, true, false, true];
      ctx.save();
      ctx.globalAlpha = out;
      ctx.font = mono(24);
      const cw = ctx.measureText('M').width;
      const full = words.join(' ');
      let x = 640 - (full.length * cw) / 2;
      const y = 450 - (1 - out) * 10;
      words.forEach((w, wi) => {
        const a0 = 1.42 + wi * 0.07;
        const pop = E.outBack(seg(t, a0, a0 + 0.25));
        for (let ci = 0; ci < w.length; ci++) {
          if (ci === 0) {
            if (pop <= 0) continue;
            ctx.save();
            ctx.translate(x + cw / 2, y - 8);
            ctx.scale(pop, pop);
            txt(w[0], 0, 8, mono(24, hot[wi] ? 700 : 400), hot[wi] ? C.mint : C.dim, 'center');
            ctx.restore();
          } else {
            const at = 1.62 + wi * 0.05 + ci * 0.022;
            if (t >= at) txt(w[ci], x + ci * cw, y, mono(24), C.fg);
          }
        }
        x += (w.length + 1) * cw;
      });
      const sub = life(t, 1.85, 2.45, 0.3, 0.2);
      txt('a terminal git history viewer', 640, 500, mono(14), hexA(C.dim, sub), 'center');
      ctx.restore();
    }
  }

  // ---------------------------------------------------------------- S2 history
  function sceneHistory(t) {
    const out = E.inCubic(seg(t, 4.85, 5.15));
    // axis
    const ax = tw(t, 2.5, 3.1, E.inOutExpo);
    if (ax > 0) {
      ctx.strokeStyle = hexA(C.dim, 0.6 * (1 - out));
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(110, AXIS_Y + 0.5);
      ctx.lineTo(lerp(110, 1170, ax), AXIS_Y + 0.5);
      ctx.stroke();
    }
    // day labels + tag diamonds
    const tagP = seg(t, 2.95, 4.45);
    const tagIdx = Math.min(TAGS.length - 1, Math.floor(E.outCubic(tagP) * TAGS.length));
    DAYS.forEach(([label, n], d) => {
      const a = seg(t, 2.6 + d * 0.035, 2.9 + d * 0.035) * (1 - out);
      if (a <= 0) return;
      txt(label, colX(d), AXIS_Y + 26 + (1 - a) * 8, mono(11), hexA(C.dim, a), 'center');
      // tags released that day, revealed as the version roller passes them
      let shown = 0, first = -1;
      TAGS.forEach((tg, i) => { if (tg.d === d) { if (first < 0) first = i; if (i <= tagIdx && tagP > 0) shown++; } });
      if (shown > 0) {
        const p = E.outBack(seg(t, 2.95 + (first / TAGS.length) * 1.2, 3.25 + (first / TAGS.length) * 1.2));
        ctx.save();
        ctx.translate(colX(d), AXIS_Y + 46);
        ctx.globalAlpha = (1 - out);
        ctx.scale(p, p);
        ctx.rotate(Math.PI / 4);
        ctx.fillStyle = C.amber;
        ctx.fillRect(-4, -4, 8, 8);
        ctx.restore();
        if (shown > 1) txt('×' + shown, colX(d) + 9, AXIS_Y + 50, mono(10), hexA(C.amber, 0.8 * (1 - out)));
      }
    });

    // commit dots — rise per day, then match-cut into the Pick list rows
    const conv = E.inOutCubic(seg(t, 4.95, 5.55));
    for (const dt of DOTS) {
      const a0 = 2.75 + (dt.gi / DOTS.length) * 1.5 + dt.jit;
      const p = seg(t, a0, a0 + 0.35);
      if (p <= 0) continue;
      const rise = E.outBack(p);
      let x = dt.x, y = dt.y + (1 - rise) * 40;
      let r = 2.6, alpha = clamp(p * 3);
      if (conv > 0) {
        const ty = rowY(dt.tr) + ROW_H / 2;
        x = lerp(x, dt.tx, conv);
        y = lerp(y, ty, conv) + Math.sin(conv * Math.PI) * (dt.d - 7) * -6;
        alpha *= 1 - seg(t, 5.35, 5.6);
        r = lerp(2.6, 1.6, conv);
      }
      ctx.fillStyle = hexA(dt.color, alpha);
      if (conv > 0.05 && conv < 0.98) {
        // streak toward the target for a motion-blur feel
        ctx.strokeStyle = hexA(dt.color, alpha * 0.35);
        ctx.lineWidth = r * 1.6;
        ctx.beginPath();
        ctx.moveTo(x, y);
        ctx.lineTo(lerp(x, dt.x, 0.12), lerp(y, dt.y, 0.12));
        ctx.stroke();
      }
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fill();
    }

    // stats
    const sa = life(t, 2.8, 5.0, 0.3, 0.2);
    if (sa > 0) {
      const lift = (1 - E.outExpo(seg(t, 2.8, 3.4))) * 30;
      const n = Math.round(E.outExpo(seg(t, 2.85, 4.4)) * 347);
      ctx.save();
      ctx.globalAlpha = sa;
      txt(String(n), 330, 236 + lift, mono(92, 700), C.fg, 'right');
      txt('commits', 344, 236 + lift, mono(18), C.dim);
      txt('2026·05·21 → 10·01', 344, 206 + lift, mono(13), C.mint);
      // version roller
      const rollP = seg(t, 2.95, 4.45);
      const fv = E.outCubic(rollP) * (TAGS.length - 1);
      const vi = Math.floor(fv), vf = fv - vi;
      ctx.save();
      ctx.beginPath();
      ctx.rect(640, 166 + lift, 360, 78);
      ctx.clip();
      for (const [idx, off] of [[vi, -vf], [Math.min(vi + 1, TAGS.length - 1), 1 - vf]]) {
        if (idx === vi + 1 && vi === TAGS.length - 1) continue;
        txt(TAGS[idx].v, 980, 236 + lift + off * 92, mono(80, 700), C.amber, 'right');
      }
      ctx.restore();
      txt('releases', 994, 236 + lift, mono(18), C.dim);
      txt(String(Math.min(TAGS.length, vi + 1)).padStart(2, '0') + ' / 38', 994, 206 + lift, mono(13), C.amber);
      ctx.restore();
    }
  }

  // ---------------------------------------------------------------- S3 pick → view → diff
  function sceneTui(t) {
    const appear = E.outExpo(seg(t, 5.15, 5.75));
    const dim = seg(t, 8.45, 8.75);
    ctx.save();
    ctx.globalAlpha = appear * (1 - dim * 0.7);
    const sc = lerp(0.965, 1, appear) - dim * 0.04;
    ctx.translate(640, 370);
    ctx.scale(sc, sc);
    ctx.translate(-640, -370);

    // window chrome
    ctx.shadowColor = 'rgba(0,0,0,0.6)';
    ctx.shadowBlur = 50;
    ctx.shadowOffsetY = 20;
    rr(WIN.x, WIN.y, WIN.w, WIN.h, 14);
    ctx.fillStyle = C.panel;
    ctx.fill();
    ctx.shadowColor = 'transparent';
    ctx.strokeStyle = C.line;
    ctx.lineWidth = 1;
    ctx.stroke();
    [C.rose, C.amber, C.mint].forEach((c, i) => {
      ctx.fillStyle = hexA(c, 0.85);
      ctx.beginPath();
      ctx.arc(WIN.x + 22 + i * 18, WIN.y + 24, 5, 0, Math.PI * 2);
      ctx.fill();
    });
    txt('glc  ~/Repository/gluck', WIN.x + 84, WIN.y + 29, mono(13), C.dim);
    ctx.fillStyle = C.line;
    ctx.fillRect(WIN.x, WIN.y + 46, WIN.w, 1);

    // mode pills
    const modes = ['PICK', 'VIEW', 'DIFF'];
    const mp = tw(t, 6.5, 6.85, E.inOutExpo) + tw(t, 7.5, 7.85, E.inOutExpo);
    const px = i => WIN.x + WIN.w - 228 + i * 72;
    rr(lerp(px(0), px(1), Math.min(mp, 1)) + (mp > 1 ? (px(2) - px(1)) * (mp - 1) : 0), WIN.y + 12, 64, 24, 12);
    ctx.fillStyle = hexA(C.mint, 0.18);
    ctx.fill();
    ctx.strokeStyle = hexA(C.mint, 0.7);
    ctx.stroke();
    modes.forEach((m, i) => {
      const on = Math.abs(mp - i) < 0.5;
      txt(m, px(i) + 32, WIN.y + 29, mono(12, 600), on ? C.mint : C.dim, 'center');
    });

    // panel frames
    [LP, RP].forEach(p => {
      rr(p.x, p.y, p.w, p.h, 8);
      ctx.fillStyle = C.bg2;
      ctx.fill();
      ctx.strokeStyle = hexA(C.line, 0.9);
      ctx.stroke();
    });

    // ---- left panel: commit rows → file tree
    ctx.save();
    rr(LP.x, LP.y, LP.w, LP.h, 8);
    ctx.clip();
    const toView = seg(t, 6.5, 6.75);
    let sel = 0;
    [5.6, 5.85, 6.1].forEach(k => { sel += E.outExpo(seg(t, k, k + 0.18)); });
    if (toView < 1) {
      const rowsIn = seg(t, 5.3, 5.6);
      ctx.save();
      ctx.globalAlpha = rowsIn * (1 - toView);
      ctx.translate(-toView * 30, 0);
      rr(LP.x + 6, rowY(0) + sel * ROW_H + 2, LP.w - 12, ROW_H - 4, 6);
      ctx.fillStyle = hexA(C.mint, 0.14);
      ctx.fill();
      ctx.fillStyle = C.mint;
      ctx.fillRect(LP.x + 6, rowY(0) + sel * ROW_H + 8, 3, ROW_H - 16);
      COMMITS.forEach(([h, m], r) => {
        const y = rowY(r) + 22;
        const on = Math.round(sel) === r;
        txt(h, LP.x + 18, y, mono(13), on ? C.amber : hexA(C.amber, 0.6));
        txt(m, LP.x + 92, y, sans(14, on ? 600 : 400), on ? C.fg : hexA(C.fg, 0.7));
      });
      ctx.restore();
    }
    if (toView > 0) {
      TREE.forEach(([name, depth, color, hot], i) => {
        const a = E.outExpo(seg(t, 6.6 + i * 0.022, 6.9 + i * 0.022));
        if (a <= 0) return;
        const y = rowY(i) + 22;
        ctx.save();
        ctx.globalAlpha = a;
        ctx.translate((1 - a) * 40, 0);
        if (i === TREE_SEL) {
          rr(LP.x + 6, rowY(i) + 2, LP.w - 12, ROW_H - 4, 6);
          ctx.fillStyle = hexA(C.mint, 0.14);
          ctx.fill();
          ctx.fillStyle = C.mint;
          ctx.fillRect(LP.x + 6, rowY(i) + 8, 3, ROW_H - 16);
        }
        txt(name, LP.x + 20 + depth * 20, y, mono(14, i === TREE_SEL ? 600 : 400),
          color || (i === TREE_SEL ? C.fg : hexA(C.fg, 0.75)));
        if (hot) txt('*', LP.x + 20 + depth * 20 + name.length * 8.6 + 6, y, mono(14, 700), C.amber);
        ctx.restore();
      });
    }
    ctx.restore();

    // ---- right panel
    ctx.save();
    rr(RP.x, RP.y, RP.w, RP.h, 8);
    ctx.clip();
    // pick: abstract diff preview, reshuffled per selected commit
    if (toView < 1) {
      const si = Math.round(sel);
      const swap = Math.min(1, ...[5.6, 5.85, 6.1].map(k => (t >= k ? seg(t, k, k + 0.22) : 1)));
      const R = rng(100 + si);
      ctx.save();
      ctx.globalAlpha = seg(t, 5.35, 5.7) * (1 - toView);
      txt(['src/search/typo.rs', 'Cargo.toml', 'site/index.html', 'src/search/bm25.rs'][si] || '', RP.x + 18, RP.y + 28, mono(13), C.blue);
      txt('+' + (12 + si * 7) + ' −' + (3 + si * 2), RP.x + RP.w - 18, RP.y + 28, mono(12), C.dim, 'right');
      for (let i = 0; i < 13; i++) {
        const y = RP.y + 52 + i * 29;
        const kind = R();
        const a = E.outExpo(seg(swap, i * 0.04, i * 0.04 + 0.5));
        const tint = kind < 0.22 ? C.rose : kind < 0.5 ? C.mint : null;
        if (tint) {
          ctx.fillStyle = hexA(tint, 0.08 * a);
          ctx.fillRect(RP.x + 8, y - 4, RP.w - 16, 24);
          txt(tint === C.rose ? '−' : '+', RP.x + 20, y + 13, mono(13, 700), hexA(tint, a));
        }
        let x = RP.x + 40 + Math.floor(R() * 3) * 22;
        const segs = 2 + Math.floor(R() * 4);
        for (let s = 0; s < segs; s++) {
          const w = (24 + R() * 90) * a;
          ctx.fillStyle = hexA([C.violet, C.blue, C.fg, C.amber, C.dim][Math.floor(R() * 5)], 0.55);
          rr(x, y + 4, w, 9, 4.5);
          ctx.fill();
          x += w + 9;
        }
      }
      ctx.restore();
    }
    // view + diff: real code
    if (toView > 0) {
      const split = tw(t, 7.5, 7.88, E.inOutExpo);
      const half = (RP.w - 12) / 2;
      const nx = lerp(RP.x, RP.x + half + 12, split), nw = lerp(RP.w, half, split);
      const fsz = lerp(13, 11.5, split);
      const lh = 26;
      const pane = (x, w, lines, side, a) => {
        ctx.save();
        ctx.beginPath();
        ctx.rect(x, RP.y, w, RP.h);
        ctx.clip();
        txt(side === 'old' ? 'rrf.rs @ 5d15db5' : side === 'diff' ? 'rrf.rs @ HEAD' : 'src/search/rrf.rs',
          x + 16, RP.y + 26, mono(12), hexA(side === 'old' ? C.rose : C.blue, a));
        lines.forEach((ln, i) => {
          const la = side === 'old' ? a : E.outExpo(seg(t, 6.72 + i * 0.03, 7.0 + i * 0.03));
          if (la <= 0) return;
          const y = RP.y + 56 + i * lh;
          const changed = CHANGED.has(i) && side !== 'view';
          const hl = seg(t, 7.8, 8.1);
          if (changed) {
            ctx.fillStyle = hexA(side === 'old' ? C.rose : C.mint, 0.13 * hl);
            ctx.fillRect(x + 2, y - 15, w - 4, lh - 4);
            ctx.fillStyle = hexA(side === 'old' ? C.rose : C.mint, hl);
            ctx.fillRect(x + 2, y - 15, 2, lh - 4);
          }
          ctx.save();
          ctx.globalAlpha = la;
          ctx.translate((1 - la) * 16, 0);
          txt(String(47 + i), x + 34, y, mono(11), C.faint, 'right');
          if (ln) drawCode(ln, x + 46, y, fsz);
          ctx.restore();
        });
        ctx.restore();
      };
      ctx.save();
      ctx.globalAlpha = seg(t, 6.6, 6.8);
      pane(nx, nw, CODE_NEW, split > 0 ? 'diff' : 'view', 1);
      if (split > 0) {
        ctx.globalAlpha = split;
        pane(RP.x - (1 - split) * 40, half, CODE_OLD, 'old', split);
        ctx.fillStyle = hexA(C.line, split);
        ctx.fillRect(RP.x + half + 5, RP.y + 10, 1, RP.h - 20);
      }
      ctx.restore();
      // scanline that sweeps as the file opens
      const sw = seg(t, 6.7, 7.15);
      if (sw > 0 && sw < 1) {
        const y = RP.y + sw * RP.h;
        const g = ctx.createLinearGradient(0, y - 60, 0, y);
        g.addColorStop(0, hexA(C.mint, 0));
        g.addColorStop(1, hexA(C.mint, 0.18));
        ctx.fillStyle = g;
        ctx.fillRect(RP.x, y - 60, RP.w, 60);
        ctx.fillStyle = hexA(C.mint, 0.7);
        ctx.fillRect(RP.x, y, RP.w, 1);
      }
    }
    ctx.restore();

    // footer hints
    const hint = t < 6.5 ? 'j/k move   enter view   tab diff   s search'
      : t < 7.5 ? 'h/l fold   . gitignore   w wrap   tab diff'
        : 'v unified   h/l files   w wrap   esc back';
    txt(hint, WIN.x + 20, WIN.y + WIN.h - 12, mono(12), C.dim);
    ctx.restore();
  }

  // ---------------------------------------------------------------- S4 semantic search
  function bez(p0, p1, p2, p3, s) {
    const u = 1 - s;
    return [
      u * u * u * p0[0] + 3 * u * u * s * p1[0] + 3 * u * s * s * p2[0] + s * s * s * p3[0],
      u * u * u * p0[1] + 3 * u * u * s * p1[1] + 3 * u * s * s * p2[1] + s * s * s * p3[1],
    ];
  }
  function flow(a, b, p, color, t, alpha) {
    if (p <= 0) return;
    const mx = (a[0] + b[0]) / 2;
    const c1 = [mx, a[1]], c2 = [mx, b[1]];
    ctx.strokeStyle = hexA(color, 0.55 * alpha);
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    for (let i = 0; i <= 40; i++) {
      const q = bez(a, c1, c2, b, (i / 40) * p);
      i ? ctx.lineTo(q[0], q[1]) : ctx.moveTo(q[0], q[1]);
    }
    ctx.stroke();
    if (p >= 1) {
      for (let k = 0; k < 3; k++) {
        const s = ((t * 1.4 + k / 3) % 1);
        const q = bez(a, c1, c2, b, s);
        ctx.fillStyle = hexA(color, alpha * Math.sin(s * Math.PI));
        ctx.beginPath();
        ctx.arc(q[0], q[1], 3, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }
  function node(x, y, w, h, title, color, p) {
    if (p <= 0) return;
    const s = E.outBack(p);
    ctx.save();
    ctx.translate(x + w / 2, y + h / 2);
    ctx.scale(s, s);
    ctx.translate(-(x + w / 2), -(y + h / 2));
    rr(x, y, w, h, 10);
    ctx.fillStyle = C.panel2;
    ctx.fill();
    ctx.strokeStyle = hexA(color, 0.8);
    ctx.lineWidth = 1.2;
    ctx.stroke();
    txt(title, x + 14, y + 22, mono(12, 600), color);
    ctx.restore();
  }

  function sceneSearch(t) {
    const M = { x: 220, y: 128, w: 840, h: 476 };
    const pop = seg(t, 8.5, 8.85);
    if (pop <= 0) return;
    ctx.save();
    const s = lerp(0.9, 1, E.outBack(pop));
    ctx.globalAlpha = clamp(pop * 3);
    ctx.translate(640, 366);
    ctx.scale(s, s);
    ctx.translate(-640, -366);
    ctx.shadowColor = hexA(C.violet, 0.35);
    ctx.shadowBlur = 60;
    rr(M.x, M.y, M.w, M.h, 16);
    ctx.fillStyle = '#0e131a';
    ctx.fill();
    ctx.shadowBlur = 0;
    ctx.strokeStyle = hexA(C.violet, 0.6);
    ctx.lineWidth = 1.2;
    ctx.stroke();
    txt('semantic search', M.x + 26, M.y + 34, mono(13, 600), C.violet);
    txt('commits · files · symbols', M.x + M.w - 26, M.y + 34, mono(12), C.dim, 'right');

    // input
    rr(M.x + 24, M.y + 52, M.w - 48, 50, 10);
    ctx.fillStyle = C.bg2;
    ctx.fill();
    ctx.strokeStyle = hexA(C.mint, 0.5);
    ctx.stroke();
    txt('›', M.x + 44, M.y + 85, mono(22, 700), C.mint);
    const qn = Math.floor(seg(t, 8.85, 9.5) * QUERY.length + 0.0001);
    const q = QUERY.slice(0, qn);
    txt(q, M.x + 70, M.y + 85, sans(21, 500), C.fg);
    ctx.font = sans(21, 500);
    const qw = ctx.measureText(q).width;
    if (t < 9.6 || Math.floor(t * 2.5) % 2 === 0) {
      ctx.fillStyle = C.mint;
      ctx.fillRect(M.x + 72 + qw, M.y + 64, 2, 26);
    }

    // pipeline
    const pipeOut = E.inCubic(seg(t, 10.35, 10.6));
    if (pipeOut < 1) {
      ctx.save();
      ctx.globalAlpha *= 1 - pipeOut;
      ctx.translate(0, -pipeOut * 30);
      const qy = 392;
      const qp = E.outBack(seg(t, 9.45, 9.7));
      if (qp > 0) {
        ctx.save();
        ctx.translate(330, qy);
        ctx.scale(qp, qp);
        rr(-56, -20, 112, 40, 20);
        ctx.fillStyle = hexA(C.mint, 0.16);
        ctx.fill();
        ctx.strokeStyle = C.mint;
        ctx.stroke();
        txt('query', 0, 5, mono(14, 600), C.mint, 'center');
        ctx.restore();
      }
      const bm = { x: 520, y: 256, w: 290, h: 92 }, vc = { x: 520, y: 438, w: 290, h: 92 };
      flow([386, qy], [bm.x, bm.y + bm.h / 2], tw(t, 9.55, 9.85, E.inOutCubic), C.blue, t, 1);
      flow([386, qy], [vc.x, vc.y + vc.h / 2], tw(t, 9.6, 9.9, E.inOutCubic), C.violet, t, 1);
      node(bm.x, bm.y, bm.w, bm.h, 'BM25 · char bigram', C.blue, seg(t, 9.7, 9.95));
      node(vc.x, vc.y, vc.w, vc.h, 'Vector · 256-dim', C.violet, seg(t, 9.75, 10.0));
      BIGRAMS.forEach((b, i) => {
        const p = E.outBack(seg(t, 9.85 + i * 0.035, 10.05 + i * 0.035));
        if (p <= 0) return;
        const x = bm.x + 14 + i * 45, y = bm.y + 42;
        ctx.save();
        ctx.translate(x + 20, y + 15);
        ctx.scale(p, p);
        rr(-20, -14, 40, 28, 6);
        ctx.fillStyle = hexA(C.blue, 0.16);
        ctx.fill();
        txt(b, 0, 5, sans(13, 600), C.fg, 'center');
        ctx.restore();
      });
      const vp = seg(t, 9.85, 10.15);
      if (vp > 0) {
        const R = rng(9);
        for (let i = 0; i < 56; i++) {
          const base = R();
          const h = (8 + base * 34) * E.outExpo(clamp(vp * 2 - i / 56)) * (0.75 + 0.25 * Math.sin(t * 9 + i));
          ctx.fillStyle = hexA(i % 7 === 0 ? C.mint : C.violet, 0.8);
          ctx.fillRect(vc.x + 14 + i * 4.6, vc.y + 78 - h, 2.6, h);
        }
      }
      const rx = 950, ry = qy;
      flow([bm.x + bm.w, bm.y + bm.h / 2], [rx - 42, ry], tw(t, 10.0, 10.25, E.inOutCubic), C.blue, t, 1);
      flow([vc.x + vc.w, vc.y + vc.h / 2], [rx - 42, ry], tw(t, 10.02, 10.27, E.inOutCubic), C.violet, t, 1);
      const rp = E.outBack(seg(t, 10.2, 10.4));
      if (rp > 0) {
        for (let k = 0; k < 2; k++) {
          const ring = seg(t, 10.25 + k * 0.12, 10.65 + k * 0.12);
          if (ring > 0 && ring < 1) {
            ctx.strokeStyle = hexA(C.amber, 1 - ring);
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.arc(rx, ry, 42 + ring * 50, 0, Math.PI * 2);
            ctx.stroke();
          }
        }
        ctx.save();
        ctx.translate(rx, ry);
        ctx.scale(rp, rp);
        ctx.fillStyle = hexA(C.amber, 0.16);
        ctx.strokeStyle = C.amber;
        ctx.beginPath();
        ctx.arc(0, 0, 42, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
        txt('RRF', 0, 0, mono(16, 700), C.amber, 'center');
        txt('k=60', 0, 18, mono(11), C.dim, 'center');
        ctx.restore();
      }
      ctx.restore();
    }

    // results
    RESULTS.forEach(([kind, id, title, score], i) => {
      const a0 = 10.5 + i * 0.07;
      const p = E.outExpo(seg(t, a0, a0 + 0.45));
      if (p <= 0) return;
      const y = M.y + 124 + i * 66;
      ctx.save();
      ctx.globalAlpha *= p;
      ctx.translate((1 - p) * 60, 0);
      rr(M.x + 24, y, M.w - 48, 56, 10);
      ctx.fillStyle = i === 0 ? hexA(C.mint, 0.12) : hexA('#ffffff', 0.025);
      ctx.fill();
      if (i === 0) {
        ctx.fillStyle = C.mint;
        ctx.fillRect(M.x + 24, y + 12, 3, 32);
      }
      txt(String(i + 1), M.x + 50, y + 35, mono(18, 700), i === 0 ? C.mint : C.dim, 'center');
      const kc = kind === 'commit' ? C.amber : kind === 'symbol' ? C.violet : C.blue;
      rr(M.x + 74, y + 17, 66, 22, 11);
      ctx.fillStyle = hexA(kc, 0.16);
      ctx.fill();
      txt(kind, M.x + 107, y + 32, mono(11, 600), kc, 'center');
      txt(id, M.x + 154, y + 33, mono(13), kc);
      txt(title, M.x + 270, y + 34, sans(15, i === 0 ? 600 : 400), i === 0 ? C.fg : hexA(C.fg, 0.8));
      const bw = 90 * score * E.outExpo(seg(t, a0 + 0.15, a0 + 0.7));
      ctx.fillStyle = hexA(C.line, 1);
      ctx.fillRect(M.x + M.w - 130, y + 27, 90, 3);
      ctx.fillStyle = i === 0 ? C.mint : hexA(C.mint, 0.5);
      ctx.fillRect(M.x + M.w - 130, y + 27, bw, 3);
      ctx.restore();
    });
    ctx.restore();
  }

  // ---------------------------------------------------------------- wipe (S4 → S5)
  const WIPE_SWITCH = 11.68;
  function wipe(t) {
    const cols = [C.mint, C.blue, C.violet, C.amber];
    const bh = H / cols.length;
    cols.forEach((c, i) => {
      const cin = E.inOutExpo(seg(t, 11.25 + i * 0.04, 11.55 + i * 0.04));
      const cout = E.inOutExpo(seg(t, 11.7 + i * 0.045, 12.02 + i * 0.045));
      if (cin <= 0 || cout >= 1) return;
      const L = lerp(-120, W + 120, cout), R = lerp(-120, W + 120, cin);
      const y0 = i * bh - 1, y1 = (i + 1) * bh + 1, sk = 70;
      ctx.fillStyle = c;
      ctx.beginPath();
      ctx.moveTo(L + sk, y0);
      ctx.lineTo(R + sk, y0);
      ctx.lineTo(R, y1);
      ctx.lineTo(L, y1);
      ctx.closePath();
      ctx.fill();
    });
  }

  // ---------------------------------------------------------------- S5 measured
  function sceneNumbers(t) {
    const tiles = 4, tw_ = 262, gap = 30, x0 = (W - (tiles * tw_ + (tiles - 1) * gap)) / 2, y0 = 236, th = 270;
    const ha = life(t, 11.75, 13.5, 0.3, 0.2);
    if (ha > 0) {
      ctx.save();
      ctx.globalAlpha = ha;
      txt('$', x0, 182, mono(20, 600), C.mint);
      txt('glc report', x0 + 22, 182, mono(20, 600), C.fg);
      txt('MRR · Recall@k · NDCG@10 · p50/p95/p99', x0 + tiles * tw_ + (tiles - 1) * gap, 182, mono(13), C.dim, 'right');
      ctx.restore();
    }
    const strip = life(t, 12.35, 13.45, 0.3, 0.15);
    if (strip > 0) {
      txt('16,169 lines of Rust  ·  347 commits  ·  38 releases  ·  2026', W / 2, 568 + (1 - strip) * 10, mono(14), hexA(C.dim, strip), 'center');
    }
    for (let i = 0; i < tiles; i++) {
      const a0 = 11.78 + i * 0.09;
      const p = E.outExpo(seg(t, a0, a0 + 0.6));
      if (p <= 0) continue;
      const collapse = E.inExpo(seg(t, 13.3 + i * 0.04, 13.55 + i * 0.04));
      if (collapse >= 1) continue;
      const x = x0 + i * (tw_ + gap);
      const y = y0 + (1 - p) * 80;
      ctx.save();
      ctx.globalAlpha = clamp(p * 2);
      ctx.translate(x + tw_ / 2, 360);
      ctx.scale(1, 1 - collapse);
      ctx.translate(-(x + tw_ / 2), -360);
      rr(x, y, tw_, th, 14);
      ctx.fillStyle = hexA(C.panel2, 0.92);
      ctx.fill();
      ctx.strokeStyle = C.line;
      ctx.stroke();
      const accent = [C.mint, C.blue, C.amber, C.violet][i];
      ctx.fillStyle = accent;
      ctx.fillRect(x + 22, y + 22, 18, 3);
      const k = E.outExpo(seg(t, a0 + 0.1, a0 + 1.1));
      const pad = x + 22;
      if (i === 0) {
        txt('QUALITY REPORTS', pad, y + 50, mono(12, 600), C.dim);
        txt(String(Math.round(20 * k)), pad, y + 120, mono(64, 700), C.fg);
        // MRR sparkline across all 20 reports
        const sx = pad, sy = y + 150, sw = tw_ - 44, sh = 60;
        const n = Math.max(2, Math.ceil(k * MRR.length));
        ctx.strokeStyle = accent;
        ctx.lineWidth = 2;
        ctx.beginPath();
        for (let j = 0; j < n; j++) {
          const px = sx + (j / (MRR.length - 1)) * sw, py = sy + sh - ((MRR[j] - 0.28) / 0.5) * sh;
          j ? ctx.lineTo(px, py) : ctx.moveTo(px, py);
        }
        ctx.stroke();
        const lx = sx + ((n - 1) / (MRR.length - 1)) * sw, ly = sy + sh - ((MRR[n - 1] - 0.28) / 0.5) * sh;
        ctx.fillStyle = accent;
        ctx.beginPath();
        ctx.arc(lx, ly, 4, 0, Math.PI * 2);
        ctx.fill();
        txt('MRR · 쿼리셋 7 → 63개로 확장', pad, y + th - 22, sans(12), C.dim);
      } else if (i === 1) {
        txt('EVAL QUERIES', pad, y + 50, mono(12, 600), C.dim);
        txt(String(Math.round(63 * k)), pad, y + 120, mono(64, 700), C.fg);
        CATS.forEach(([name, v], j) => {
          const by = y + 146 + j * 15;
          txt(name, pad, by + 9, mono(10), C.dim);
          ctx.fillStyle = hexA(C.line, 1);
          ctx.fillRect(pad + 62, by + 3, 150, 5);
          ctx.fillStyle = accent;
          ctx.fillRect(pad + 62, by + 3, 150 * v * E.outExpo(seg(t, a0 + 0.2 + j * 0.05, a0 + 0.9 + j * 0.05)), 5);
        });
        txt('54 positive · 9 negative', pad, y + th - 22, sans(12), C.dim);
      } else if (i === 2) {
        txt('TESTS PASSING', pad, y + 50, mono(12, 600), C.dim);
        txt(String(Math.round(383 * k)), pad, y + 120, mono(64, 700), C.fg);
        const filled = Math.floor(383 * E.outCubic(seg(t, a0 + 0.1, a0 + 1.0)));
        for (let j = 0; j < 383; j++) {
          const cx = pad + (j % 32) * 6.8, cy = y + 146 + Math.floor(j / 32) * 6.4;
          ctx.fillStyle = j < filled ? accent : hexA(C.line, 1);
          ctx.fillRect(cx, cy, 4.4, 4.4);
        }
        txt('cargo test · 0 failed', pad, y + th - 22, sans(12), C.dim);
      } else {
        txt('HIGHLIGHTED LANGS', pad, y + 50, mono(12, 600), C.dim);
        txt(String(Math.round(15 * k)), pad, y + 120, mono(64, 700), C.fg);
        // slot-style roll through language names
        const rollP = E.outCubic(seg(t, a0 + 0.1, a0 + 1.3)) * (LANGS.length - 1);
        const li = Math.floor(rollP), lf = rollP - li;
        ctx.save();
        ctx.beginPath();
        ctx.rect(pad, y + 150, tw_ - 44, 54);
        ctx.clip();
        for (let j = -1; j <= 2; j++) {
          const idx = li + j;
          if (idx < 0 || idx >= LANGS.length) continue;
          const off = (j - lf) * 30;
          const a = 1 - Math.min(1, Math.abs(off) / 40);
          txt(LANGS[idx], pad, y + 186 + off, mono(22, 600), hexA(accent, a));
        }
        ctx.restore();
        txt('tree-sitter · Asm · ld · Lisette', pad, y + th - 22, sans(12), C.dim);
      }
      ctx.restore();
    }
    // collapse line
    const ln = life(t, 13.45, 13.95, 0.08, 0.3);
    if (ln > 0) {
      const w = lerp(1100, 0, E.inOutExpo(seg(t, 13.55, 13.95)));
      ctx.save();
      ctx.shadowColor = C.mint;
      ctx.shadowBlur = 18;
      ctx.fillStyle = hexA('#ffffff', 0.95 * ln);
      ctx.fillRect(640 - w / 2, 359, w, 2);
      ctx.restore();
    }
  }

  // ---------------------------------------------------------------- S6 outro
  function sceneOutro(t) {
    const a = E.outExpo(seg(t, 13.85, 14.3));
    if (a > 0) {
      txt('커밋을 따라, 코드를 읽다.', 640, 404 + (1 - a) * 16, sans(30, 600), hexA(C.fg, a), 'center');
    }
    const c = seg(t, 14.05, 14.3);
    if (c > 0) {
      const cmd = 'brew tap soomtong/tap && brew install glc';
      ctx.font = mono(16);
      const cw = ctx.measureText(cmd).width + 64;
      const x = 640 - cw / 2, y = 446;
      ctx.save();
      ctx.globalAlpha = E.outCubic(c);
      rr(x, y, cw, 46, 23);
      ctx.fillStyle = hexA(C.panel2, 0.9);
      ctx.fill();
      ctx.strokeStyle = hexA(C.mint, 0.45);
      ctx.stroke();
      const n = Math.floor(seg(t, 14.12, 14.6) * cmd.length);
      txt('$', x + 24, y + 29, mono(16, 700), C.mint);
      txt(cmd.slice(0, n), x + 44, y + 29, mono(16), C.fg);
      ctx.font = mono(16);
      const tx = x + 44 + ctx.measureText(cmd.slice(0, n)).width;
      if (Math.floor(t * 2.5) % 2 === 0 || n < cmd.length) {
        ctx.fillStyle = C.mint;
        ctx.fillRect(tx + 3, y + 14, 9, 18);
      }
      ctx.restore();
    }
    const u = seg(t, 14.45, 14.8);
    if (u > 0) txt('github.com/soomtong/gluck', 640, 536, mono(13), hexA(C.dim, u), 'center');
  }

  // ---------------------------------------------------------------- frame
  function render(c2d, t, scaleX) {
    ctx = c2d;
    ctx.setTransform(scaleX, 0, 0, scaleX, 0, 0);
    background(t);
    if (t < 2.6) sceneBoot(t);
    if (t > 2.4 && t < 4.95) sceneHistory(t);
    if (t > 5.1 && t < WIPE_SWITCH) sceneTui(t);
    if (t >= 4.95 && t < 5.7) sceneHistory(t);
    if (t > 8.4 && t < WIPE_SWITCH) sceneSearch(t);
    if (t >= WIPE_SWITCH && t < 14) sceneNumbers(t);
    if (t > 13.8) sceneOutro(t);
    keycap(t, '⏎', 0.82);
    keycap(t, 'j', 5.6);
    keycap(t, 'j', 5.85);
    keycap(t, 'j', 6.1);
    keycap(t, 'Enter', 6.5);
    keycap(t, 'Tab', 7.5);
    keycap(t, 's', 8.45);
    wipe(t);
    drawLogo(t, scaleX);
    ctx.setTransform(scaleX, 0, 0, scaleX, 0, 0);
    hud(t);
    finish(t);
  }

  // ---------------------------------------------------------------- player
  function mount(root) {
    const canvas = root.querySelector('canvas');
    const btn = root.querySelector('[data-reel-toggle]');
    const bar = root.querySelector('[data-reel-bar]');
    const fill = root.querySelector('[data-reel-fill]');
    const time = root.querySelector('[data-reel-time]');
    const c2d = canvas.getContext('2d');
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    // ?reel-t=7.5 pins a single frame (used for still captures)
    const pin = parseFloat(new URLSearchParams(location.search).get('reel-t'));
    let t = !isNaN(pin) ? clamp(pin, 0, DUR) : reduce ? DUR : 0, playing = false, autoPaused = false, last = 0, raf = 0, started = false, scaleX = 1;

    CHAPTERS.forEach(([ct, num, name]) => {
      const m = document.createElement('span');
      m.className = 'reel-mark';
      m.style.left = (ct / DUR) * 100 + '%';
      m.title = `${num} ${name}`;
      bar.appendChild(m);
    });

    function size() {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const w = canvas.clientWidth;
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round((w * dpr * H) / W);
      scaleX = canvas.width / W;
      draw();
    }
    function draw() {
      render(c2d, t, scaleX);
      fill.style.transform = `scaleX(${t / DUR})`;
      time.textContent = `${t.toFixed(1).padStart(4, '0')} / ${DUR.toFixed(1)}`;
      btn.dataset.state = playing ? 'pause' : t >= DUR ? 'replay' : 'play';
      btn.setAttribute('aria-label', playing ? '일시정지' : t >= DUR ? '다시 재생' : '재생');
    }
    function loop(now) {
      if (!playing) return;
      t = Math.min(DUR, t + (now - last) / 1000);
      last = now;
      if (t >= DUR) playing = false;
      draw();
      if (playing) raf = requestAnimationFrame(loop);
    }
    function play() {
      if (t >= DUR) t = 0;
      playing = true;
      started = true;
      last = performance.now();
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(loop);
    }
    function pause() { playing = false; cancelAnimationFrame(raf); draw(); }

    const toggle = () => { autoPaused = false; playing ? pause() : play(); };
    btn.addEventListener('click', toggle);
    canvas.addEventListener('click', toggle);

    let dragging = false, wasPlaying = false;
    const seek = e => {
      const r = bar.getBoundingClientRect();
      t = clamp((e.clientX - r.left) / r.width) * DUR;
      draw();
    };
    bar.addEventListener('pointerdown', e => {
      dragging = true;
      wasPlaying = playing;
      pause();
      bar.setPointerCapture(e.pointerId);
      seek(e);
    });
    bar.addEventListener('pointermove', e => dragging && seek(e));
    bar.addEventListener('pointerup', () => {
      dragging = false;
      started = true;
      if (wasPlaying && t < DUR) play();
    });
    bar.addEventListener('keydown', e => {
      if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
        t = clamp(t + (e.key === 'ArrowRight' ? 1 : -1), 0, DUR);
        draw();
        e.preventDefault();
      } else if (e.key === ' ' || e.key === 'Enter') {
        playing ? pause() : play();
        e.preventDefault();
      }
    });

    new ResizeObserver(size).observe(canvas);
    size();

    if (!reduce && isNaN(pin) && 'IntersectionObserver' in window) {
      new IntersectionObserver(entries => {
        entries.forEach(en => {
          if (en.isIntersecting && (!started || autoPaused)) { autoPaused = false; play(); }
          else if (!en.isIntersecting && playing) { autoPaused = true; pause(); }
        });
      }, { threshold: 0.5 }).observe(canvas);
    }
  }

  document.querySelectorAll('[data-reel]').forEach(mount);
})();
