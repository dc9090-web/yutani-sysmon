/* @ds-bundle: {"format":4,"namespace":"CosmicApplets","components":[{"name":"PanelButton"},{"name":"RateReadout"},{"name":"TrafficGraph"},{"name":"MenuRow"},{"name":"AdapterList"},{"name":"DisplayModeControl"},{"name":"IndicatorPicker"},{"name":"SysPanelButton"},{"name":"MetricSection"},{"name":"Meter"},{"name":"MetricToggles"},{"name":"QuotaPanelButton"},{"name":"QuotaRow"},{"name":"QuotaHeader"}]} */
/* COSMIC Applets — preview helpers (no React). Formatting and graph geometry mirror the spec in each component README. */
(function () {
  var HISTORY = 60;

  /* Rate formatting: decimal SI, 3 significant figures, fixed 4-char number field so mono digits never shift. */
  var UNITS = ['B/s', 'kB/s', 'MB/s', 'GB/s'];
  function formatRate(bps) {
    if (bps == null || isNaN(bps)) return { num: '   —', unit: '' };
    var i = 0, v = Math.max(0, bps);
    while (v >= 999.5 && i < UNITS.length - 1) { v /= 1000; i++; }
    var s = i === 0 ? String(Math.round(v)) : (v < 9.995 ? v.toFixed(2) : v < 99.95 ? v.toFixed(1) : String(Math.round(v)));
    while (s.length < 4) s = ' ' + s;
    return { num: s, unit: UNITS[i] };
  }
  /* Compact form for vertical panels: max 4 chars, e.g. "12M", "860K", "1.2G", "0". */
  function formatCompact(bps) {
    if (bps == null || isNaN(bps)) return '—';
    var u = ['', 'K', 'M', 'G'], i = 0, v = Math.max(0, bps);
    while (v >= 999.5 && i < 3) { v /= 1000; i++; }
    if (i === 0) return v < 1 ? '0' : String(Math.round(v));
    return (v < 9.95 ? v.toFixed(1) : String(Math.round(v))) + u[i];
  }
  function formatBytes(b) { var r = formatRate(b); return r.num.trim() + ' ' + r.unit.replace('/s', ''); }

  /* Nice ceiling (1, 2, 5 × 10^n) shared by both series so they are comparable. */
  function niceMax(v) {
    if (v <= 0) return 1000;
    var p = Math.pow(10, Math.floor(Math.log10(v))), m = v / p;
    return (m <= 1 ? 1 : m <= 2 ? 2 : m <= 5 ? 5 : 10) * p;
  }
  function pts(series, w, h, max, pad) {
    pad = pad || 0;
    var n = series.length, step = (w - 2 * pad) / (HISTORY - 1), off = HISTORY - n;
    return series.map(function (v, i) {
      var x = pad + (i + off) * step, y = h - pad - (v == null ? 0 : v) / max * (h - 2 * pad);
      return [Math.round(x * 10) / 10, Math.round(y * 10) / 10];
    });
  }
  function linePath(p) { return p.map(function (q, i) { return (i ? 'L' : 'M') + q[0] + ' ' + q[1]; }).join(''); }
  function areaPath(p, h, pad) { pad = pad || 0; if (!p.length) return ''; return linePath(p) + 'L' + p[p.length - 1][0] + ' ' + (h - pad) + 'L' + p[0][0] + ' ' + (h - pad) + 'Z'; }

  /* Render download (area + stroke) and upload (line only) into an <svg>. grid: number of horizontal lines. */
  function drawGraph(svg, down, up, opt) {
    opt = opt || {};
    var w = opt.w, h = opt.h, pad = opt.pad || 0;
    var max = niceMax(Math.max.apply(null, down.concat(up).map(function (v) { return v || 0; })) * 1.05);
    var pd = pts(down, w, h, max, pad), pu = pts(up, w, h, max, pad), g = '';
    for (var i = 1; i <= (opt.grid || 0); i++) { var y = Math.round(h * i / (opt.grid + 1)) + 0.5; g += '<line class="g-grid" x1="0" x2="' + w + '" y1="' + y + '" y2="' + y + '"/>'; }
    svg.setAttribute('viewBox', '0 0 ' + w + ' ' + h);
    svg.setAttribute('preserveAspectRatio', 'none');
    svg.innerHTML = g + '<path class="g-down-area" d="' + areaPath(pd, h, pad) + '"/><path class="g-down-line" d="' + linePath(pd) + '" vector-effect="non-scaling-stroke"/><path class="g-up-line" d="' + linePath(pu) + '" vector-effect="non-scaling-stroke"/>';
    return max;
  }

  /* Deterministic traffic simulator: bursty download, steadier upload. */
  function sim(seed, base) {
    var s = seed || 7, d = [], u = [], burst = 0;
    function rnd() { s = (s * 16807) % 2147483647; return s / 2147483647; }
    base = base || 1;
    function next() {
      if (burst <= 0 && rnd() < 0.08) burst = 4 + Math.floor(rnd() * 10);
      var dl = burst > 0 ? (6e6 + rnd() * 9e6) * base : (1.2e5 + rnd() * 6e5) * base;
      var ul = (4e4 + rnd() * 2.2e5 + (burst > 0 ? 6e5 * rnd() : 0)) * base;
      burst--;
      d.push(dl); u.push(ul); if (d.length > HISTORY) { d.shift(); u.shift(); }
    }
    for (var i = 0; i < HISTORY; i++) next();
    return { down: d, up: u, tick: next, totals: { down: 4.21e9, up: 3.12e8 } };
  }


  /* Direction indicators. User setting `indicator` (cosmic-config), default 'chevrons'.
     B/C/D paths: pop-os/cosmic-icons (CC BY-SA 4.0). E: custom, shipped with the applet. Ink = currentColor. */
  function svg(p) { return '<svg viewBox="0 0 16 16" aria-hidden="true">' + p + '</svg>'; }
  var F = 'fill="currentColor"', ST = 'stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" fill="none"';
  var INDICATORS = [
    { id: 'arrows', name: 'Arrows', note: 'Text glyphs', d: '↓', u: '↑' },
    { id: 'triangles', name: 'Triangles', note: 'pan-down / pan-up', d: svg('<path d="M13 6L8 11L3 6H13Z" ' + F + '/>'), u: svg('<path d="M13 10L8 5L3 10H13Z" ' + F + '/>') },
    { id: 'chevrons', name: 'Chevrons', note: 'go-down / go-up · default',
      d: svg('<path ' + F + ' d="M8.61 11.8S8.37 12 8.01 12 7.41 11.8 7.41 11.8L1.22 5.6S.64 4.86 1.37 4.25C1.96 3.72 2.62 4.2 2.62 4.2L8.01 9.5 13.41 4.2S14.01 3.72 14.66 4.25C15.33 4.84 14.81 5.6 14.81 5.6L8.61 11.8Z"/>'),
      u: svg('<path ' + F + ' d="M8.61 4.2S8.37 4 8.01 4 7.41 4.2 7.41 4.2L1.22 10.4S.64 11.14 1.37 11.75C1.96 12.28 2.62 11.8 2.62 11.8L8.01 6.5 13.41 11.8S14.01 12.28 14.66 11.75C15.33 11.16 14.81 10.4 14.81 10.4L8.61 4.2Z"/>') },
    { id: 'rxtx-icons', name: 'Receive / transmit', note: 'network-receive / -transmit · best on panel size M+',
      d: svg('<path d="M6 4L10.5 9.5L15 4C14.997 4.004 13.667 4 13 4V1H8V4H6Z" ' + F + '/><path opacity=".35" d="M10 11L5.5 5.5L1 11C1.003 10.996 2.333 11 3 11V14H8V11H10Z" ' + F + '/>'),
      u: svg('<path opacity=".35" d="M6 4L10.5 9.5L15 4C14.997 4.004 13.667 4 13 4V1H8V4H6Z" ' + F + '/><path d="M10 11L5.5 5.5L1 11C1.003 10.996 2.333 11 3 11V14H8V11H10Z" ' + F + '/>') },
    { id: 'bar', name: 'Arrow to bar', note: 'Custom icon shipped with the applet',
      d: svg('<path d="M8 2v8.2M4.2 6.6 8 10.4l3.8-3.8M3 13.5h10" ' + ST + '/>'), u: svg('<path d="M8 14V5.8M4.2 9.4 8 5.6l3.8 3.8M3 2.5h10" ' + ST + '/>') },
    { id: 'rxtx', name: 'RX / TX', note: 'Mono text labels', txt: true, d: 'RX', u: 'TX' }
  ];
  var IND = {}; INDICATORS.forEach(function (i) { IND[i.id] = i; });
  var DEFAULT_INDICATOR = 'chevrons';
  var CHEV_DOWN = IND.chevrons.d, CHEV_UP = IND.chevrons.u;
  /* Rates as grid cells (arrow | number | unit). opts: {compact, off, bytes, indicator: id}. Callers add class 'txt' to the grid for text indicators (IND[id].txt). Arrows are separate cells so they align. */
  function ratesHTML(d, u, o) {
    o = o || {};
    function row(dir, v) {
      var I = IND[o.indicator] || IND[DEFAULT_INDICATOR];
      var cls = o.off ? 'ca-err' : (dir === 'd' ? 'ca-down' : 'ca-up'), ar = I[dir], n, un;
      if (o.off) { n = '\u2014'; un = ''; }
      else if (o.compact) { n = formatCompact(v); un = ''; }
      else if (o.bytes) { var b = formatRate(v); n = b.num.trim(); un = b.unit.replace('/s', ''); }
      else { var f = formatRate(v); n = f.num.trim(); un = f.unit; }
      return '<span class="ar ' + cls + '">' + ar + '</span><span class="n' + (o.off ? ' ca-err' : '') + '">' + n + '</span><span class="un">' + un + '</span>';
    }
    return row('d', d) + row('u', u);
  }

  /* Indicator picker segments (icon-only segmented control). */
  function pickerHTML(sel) {
    return INDICATORS.map(function (i) {
      var on = i.id === sel, wrap = function (h, c) { return '<span class="' + c + (i.txt ? ' t' : h.charAt(0) !== '<' ? ' g' : '') + '">' + h + '</span>'; };
      return '<button aria-pressed="' + on + '" data-v="' + i.id + '" title="' + i.name + '" aria-label="' + i.name + '">' + wrap(i.d, 'ar-d') + wrap(i.u, 'ar-u') + '</button>';
    }).join('');
  }


  /* ---------- System Monitor ---------- */
  /* Percent: integer in a fixed 4-char field ("  7%", " 23%", "100%"). */
  function formatPct(v) { if (v == null || isNaN(v)) return '   —'; var s = String(Math.round(Math.max(0, Math.min(100, v)))) + '%'; while (s.length < 4) s = ' ' + s; return s; }
  function formatTemp(c) { return c == null ? '—' : Math.round(c) + ' °C'; }
  function formatGHz(mhz) { return mhz == null ? '—' : (mhz / 1000).toFixed(mhz < 10000 ? 2 : 1) + ' GHz'; }
  function formatW(w) { return w == null ? '—' : Math.round(w) + ' W'; }
  /* Memory in binary GiB, 1 decimal ("18.4"). */
  function formatGiB(b) { return b == null ? '—' : (b / 1073741824).toFixed(1); }
  function clamp(v, a, b) { return Math.max(a, Math.min(b, v)); }

  /* Deterministic system simulator: cpu %, gpu %, mem bytes, disk r/w B/s, temps, clocks, power. */
  function sysSim(seed) {
    var s = seed || 5; function rnd() { s = (s * 16807) % 2147483647; return s / 2147483647; }
    var H = HISTORY, cpu = [], gpu = [], mem = [], dr = [], dw = [], st = { cpu: 18, gpu: 35, mem: 19.7e9, burst: 0, gb: 0 };
    var o = { cpu: cpu, gpu: gpu, mem: mem, memPct: [], dr: dr, dw: dw, memTotal: 67.3e9, swapTotal: 8.59e9, vramTotal: 21.5e9,
      cpuName: 'AMD Ryzen 9 7950X', cores: 16, threads: 32, gpuName: 'AMD Radeon RX 7900 XT', disk: 'nvme0n1' };
    function push(a, v) { a.push(v); if (a.length > H) a.shift(); }
    function next() {
      st.cpu = clamp(st.cpu + (rnd() - 0.5) * 14 + (rnd() < 0.05 ? 40 : 0), 3, 100); st.cpu += (14 - st.cpu) * 0.08;
      if (st.gb <= 0 && rnd() < 0.04) st.gb = 8 + Math.floor(rnd() * 12);
      st.gpu = clamp((st.gb > 0 ? 88 : 6) + (rnd() - 0.5) * 12, 0, 100); st.gb--;
      st.mem = clamp(st.mem + (rnd() - 0.48) * 2.5e8, 12e9, 60e9);
      if (st.burst <= 0 && rnd() < 0.07) st.burst = 3 + Math.floor(rnd() * 6);
      push(cpu, st.cpu); push(gpu, st.gpu); push(mem, st.mem); push(o.memPct, st.mem / o.memTotal * 100);
      push(dr, st.burst > 0 ? (2e8 + rnd() * 1.2e9) : rnd() * 3e6); push(dw, st.burst > 0 ? rnd() * 4e8 : rnd() * 1.5e6); st.burst--;
      var c = cpu[cpu.length - 1], g = gpu[gpu.length - 1];
      o.now = { cpu: c, cpuW: 38 + c * 1.6, cpuTemp: 42 + c * 0.42, cpuMHz: 3600 + c * 14, gpu: g, gpuTemp: 38 + g * 0.42, gpuHot: 44 + g * 0.55,
        gpuMHz: g > 20 ? 2380 + rnd() * 120 : 500 + rnd() * 300, gpuW: 18 + g * 2.6, gpuCap: 315, vram: (g > 20 ? 9.8e9 : 2.1e9),
        mem: st.mem, swap: 0.42e9, dr: dr[dr.length - 1], dw: dw[dw.length - 1], nvmeTemp: 41 + (st.burst > 0 ? 8 : 0) };
    }
    for (var i = 0; i < H; i++) next();
    o.tick = next; o.totals = { dr: 38.2e9, dw: 11.6e9 };
    return o;
  }
  /* Panel metric chunk: label line (metric colour) over a value line; disk = R/W pair. */
  function metricChunkHTML(m, now, o) {
    o = o || {};
    var lab = { cpu: 'CPU', gpu: 'GPU', mem: 'RAM', disk: 'DISK' }[m];
    if (m === 'disk') {
      return '<span class="lbl m-disk">' + lab + '</span><span class="rw"><span class="k ca-down">R</span><span class="v">' + formatCompact(now.dr) + '</span><span class="k ca-up">W</span><span class="v">' + formatCompact(now.dw) + '</span></span>';
    }
    var pct = m === 'mem' ? now.mem / o.memTotal * 100 : now[m];
    var warn = (m === 'cpu' && now.cpuTemp >= 85) || (m === 'gpu' && now.gpuHot >= 95);
    return '<span class="lbl m-' + m + '">' + lab + '</span><span class="v' + (warn ? ' ca-warn' : '') + '">' + formatPct(pct) + '</span>';
  }


  /* Single-series graph (percent metrics: fixed 0–100 scale; grid at 50%). */
  function drawSeries(svg, data, opt) {
    var w = opt.w, h = opt.h, pad = opt.pad || 0, max = opt.max || niceMax(Math.max.apply(null, data.map(function (v) { return v || 0; })) * 1.05);
    var p = pts(data, w, h, max, pad), g = '';
    for (var i = 1; i <= (opt.grid || 0); i++) { var y = Math.round(h * i / (opt.grid + 1)) + 0.5; g += '<line class="g-grid" x1="0" x2="' + w + '" y1="' + y + '" y2="' + y + '"/>'; }
    svg.setAttribute('viewBox', '0 0 ' + w + ' ' + h); svg.setAttribute('preserveAspectRatio', 'none');
    svg.innerHTML = g + '<path class="g-' + opt.cls + '-area" d="' + areaPath(p, h, pad) + '"/><path class="g-' + opt.cls + '-line" d="' + linePath(p) + '" vector-effect="non-scaling-stroke"/>';
    return max;
  }
  function meterHTML(cls, k, v, frac, warn) {
    return '<div class="ca-meter ' + cls + (warn ? ' warn' : '') + '"><span class="k">' + k + '</span><span class="v">' + v + '</span><span class="track" role="meter" aria-label="' + k + '" aria-valuenow="' + Math.round(frac * 100) + '" aria-valuemin="0" aria-valuemax="100"><i style="width:' + clamp(frac * 100, 0, 100).toFixed(1) + '%"></i></span></div>';
  }
  function stat(k, v, warn) { return '<div class="ca-stat"><span class="k">' + k + (warn ? ' \u00b7 Hot' : '') + '</span><span class="v' + (warn ? ' ca-warn' : '') + '">' + v + '</span></div>'; }
  /* Popup section per metric. opt: {gpuMissing, sensorsMissing, indicator} */
  function sysSectionHTML(m, o, opt) {
    opt = opt || {}; var n = o.now, h = '';
    if (m === 'cpu') {
      h = '<div class="top"><div class="title"><span class="ca-heading"><span class="dot m-cpu"></span>CPU</span><span class="ca-caption">' + o.cpuName + ' · ' + o.cores + ' cores / ' + o.threads + ' threads</span></div><div class="big">' + formatPct(n.cpu).trim() + '</div></div>' +
        '<div class="ca-mgraph"><svg data-g="cpu"></svg></div>' +
        '<div class="ca-stats">' + stat('Clock (avg)', formatGHz(n.cpuMHz)) + stat('Tctl', opt.sensorsMissing ? '—' : formatTemp(n.cpuTemp), n.cpuTemp >= 85) + stat('Package power', opt.cpuPowerMissing ? '\u2014' : formatW(n.cpuW)) + '</div>';
    } else if (m === 'gpu') {
      if (opt.gpuMissing) return '<div class="top"><div class="title"><span class="ca-heading"><span class="dot m-gpu"></span>GPU</span><span class="ca-caption">No supported GPU found · AMD (amdgpu) only</span></div><div class="big">—</div></div>';
      h = '<div class="top"><div class="title"><span class="ca-heading"><span class="dot m-gpu"></span>GPU</span><span class="ca-caption">' + o.gpuName + ' · amdgpu</span></div><div class="big">' + formatPct(n.gpu).trim() + '</div></div>' +
        '<div class="ca-mgraph"><svg data-g="gpu"></svg></div>' +
        '<div class="ca-meters">' + meterHTML('gpu', 'VRAM', formatGiB(n.vram) + ' / ' + formatGiB(o.vramTotal) + ' GiB', n.vram / o.vramTotal) + meterHTML('gpu', 'Power', formatW(n.gpuW) + ' / ' + formatW(n.gpuCap), n.gpuW / n.gpuCap) + '</div>' +
        '<div class="ca-stats">' + stat('Clock', formatGHz(n.gpuMHz)) + stat('Edge', formatTemp(n.gpuTemp), n.gpuTemp >= 85) + stat('Junction', formatTemp(n.gpuHot), n.gpuHot >= 95) + '</div>';
    } else if (m === 'mem') {
      h = '<div class="top"><div class="title"><span class="ca-heading"><span class="dot m-mem"></span>Memory</span><span class="ca-caption">' + formatGiB(o.memTotal) + ' GiB total</span></div><div class="big">' + formatGiB(n.mem) + '<span class="u">GiB used</span></div></div>' +
        '<div class="ca-mgraph"><svg data-g="mem"></svg></div>' +
        '<div class="ca-meters">' + meterHTML('mem', 'RAM', formatPct(n.mem / o.memTotal * 100).trim(), n.mem / o.memTotal) + meterHTML('mem', 'Swap', formatGiB(n.swap) + ' / ' + formatGiB(o.swapTotal) + ' GiB', n.swap / o.swapTotal) + '</div>';
    } else if (m === 'disk') {
      var r = formatRate(n.dr), w = formatRate(n.dw);
      h = '<div class="top"><div class="title"><span class="ca-heading"><span class="dot m-disk"></span>Disk I/O</span><span class="ca-caption">' + (opt.diskLabel || 'All drives') + '</span></div></div>' +
        '<div class="ca-readouts"><div class="ca-readout"><div class="lbl ca-caption"><span class="ca-swatch-down"></span>Read</div><div class="val">' + r.num.trim() + '<span class="u">' + r.unit + '</span></div></div>' +
        '<div class="ca-readout"><div class="lbl ca-caption"><span class="ca-swatch-up"></span>Write</div><div class="val">' + w.num.trim() + '<span class="u">' + w.unit + '</span></div></div></div>' +
        '<div class="ca-mgraph"><svg data-g="disk"></svg><span class="scale" data-s="disk"></span></div>' +
        '<div class="ca-stats">' + stat('Read since login', formatBytes(o.totals.dr)) + stat('Written', formatBytes(o.totals.dw)) + stat('NVMe temp', opt.sensorsMissing ? '—' : formatTemp(n.nvmeTemp)) + '</div>';
    }
    return h;
  }
  function paintSysGraphs(root, o) {
    [].forEach.call(root.querySelectorAll('svg[data-g]'), function (g) {
      var m = g.dataset.g;
      if (m === 'disk') { var mx = drawGraph(g, o.dr, o.dw, { w: 304, h: 64, grid: 1, pad: 1 }); var sc = root.querySelector('[data-s="disk"]'); if (sc) { var f = formatRate(mx); sc.textContent = f.num.trim() + ' ' + f.unit; } }
      else drawSeries(g, m === 'mem' ? o.memPct : o[m], { w: 304, h: 64, grid: 1, pad: 1, max: 100, cls: m });
    });
  }


  /* ---------- AI Usage (Claude) ---------- */
  var QH = 3600e3, QD = 24 * QH;
  /* Windows as the usage endpoint reports them: utilization % + resets_at; window length is known per kind. */
  function quotaSim(now) {
    now = now || Date.now();
    return {
      plan: 'Max', email: 'dc@example.com', updatedAt: now - 2 * 60e3, nextRefresh: now + 3 * 60e3,
      windows: [
        { key: 'session', name: 'Session', label: '5h', sub: '5-hour window', used: 42, resetAt: now + 2 * QH + 13 * 60e3, length: 5 * QH },
        { key: 'weekly', name: 'Weekly', label: 'Week', sub: 'All models · 7 days', used: 61, resetAt: now + 3 * QD + 4 * QH, length: 7 * QD },
        { key: 'fable', name: 'Fable', label: 'Fable', sub: 'Fable · 7 days', used: 84, resetAt: now + 3 * QD + 4 * QH, length: 7 * QD }
      ]
    };
  }
  function pad2(n) { return (n < 10 ? '0' : '') + n; }
  /* Reset label. mode 'relative' -> "2h 13m" / "3d 4h" / "13m" / "now"; 'absolute' -> "15:40" / "tomorrow 09:00" / "Sat 09:00" / "12 Oct 09:00". */
  function formatReset(resetAt, now, mode, compact) {
    var ms = resetAt - now;
    if (ms <= 0) return compact ? 'now' : 'now';
    if (mode === 'absolute') {
      var r = new Date(resetAt), n = new Date(now), t = pad2(r.getHours()) + ':' + pad2(r.getMinutes());
      var d0 = new Date(n.getFullYear(), n.getMonth(), n.getDate()), d1 = new Date(r.getFullYear(), r.getMonth(), r.getDate()), days = Math.round((d1 - d0) / QD);
      if (days === 0) return t; if (days === 1) return 'tomorrow ' + t;
      if (days < 7) return ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'][r.getDay()] + ' ' + t;
      return r.getDate() + ' ' + ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'][r.getMonth()] + ' ' + t;
    }
    var m = Math.floor(ms / 60e3), d = Math.floor(m / 1440), h = Math.floor((m % 1440) / 60), mm = m % 60;
    var sp = compact ? '' : ' ';
    if (d > 0) return d + 'd' + sp + h + 'h';
    if (h > 0) return h + 'h' + sp + pad2(mm) + 'm';
    return mm + 'm';
  }
  function resetPhrase(resetAt, now, mode) { var f = formatReset(resetAt, now, mode); return f === 'now' ? 'Resetting now' : (mode === 'absolute' ? 'Resets ' + f : 'Resets in ' + f); }
  /* Pace: expected % if spending evenly; |delta| < 3 = on pace. */
  function pace(w, now) {
    var elapsed = w.length - (w.resetAt - now); if (elapsed < 0) elapsed = 0;
    var exp = clamp(elapsed / w.length * 100, 0, 100), delta = w.used - exp;
    return { expected: exp, delta: delta, label: Math.abs(delta) < 3 ? 'On pace' : Math.round(Math.abs(delta)) + '% ' + (delta > 0 ? 'ahead of pace' : 'under pace') };
  }
  function quotaLevel(used) { return used >= 100 ? 'full' : used >= 80 ? 'warn' : ''; }
  /* Popup row. opt: {amount:'used'|'left', reset:'relative'|'absolute', stale} */
  function quotaRowHTML(w, now, opt) {
    opt = opt || {}; var left = opt.amount === 'left', lvl = quotaLevel(w.used), p = pace(w, now);
    var shown = left ? 100 - w.used : w.used, marker = left ? 100 - p.expected : p.expected;
    var reset = resetPhrase(w.resetAt, now, opt.reset);
    var foot = lvl === 'full' ? '<span class="ca-caption ca-err">Limit reached · ' + reset.charAt(0).toLowerCase() + reset.slice(1) + '</span><span></span>'
      : '<span class="ca-caption qreset"><span class="ca-icon i-timer"></span>' + reset + '</span><span class="ca-caption' + (p.delta >= 3 && lvl ? ' ca-warn' : '') + '">' + p.label + '</span>';
    return '<div class="ca-quota' + (lvl ? ' ' + lvl : '') + (opt.stale ? ' stale' : '') + '" data-k="' + w.key + '">' +
      '<div class="qh"><span class="title"><span class="ca-heading">' + w.name + '</span><span class="ca-caption">' + w.sub + '</span></span>' +
      '<span class="qv">' + Math.round(shown) + '%<span class="u">' + (left ? 'left' : 'used') + '</span></span></div>' +
      '<div class="qbar" role="meter" aria-label="' + w.name + ' ' + (left ? 'left' : 'used') + '" aria-valuenow="' + Math.round(shown) + '" aria-valuemin="0" aria-valuemax="100"><i style="width:' + clamp(shown, 0, 100).toFixed(1) + '%"></i>' +
      (lvl === 'full' ? '' : '<b class="pace" style="left:' + marker.toFixed(1) + '%" title="Expected at even pace: ' + Math.round(marker) + '%"></b>') + '</div>' +
      '<div class="qf">' + foot + '</div></div>';
  }
  /* Panel chunk. style: 'percent'|'bars'|'both' */
  function quotaChunkHTML(w, now, style, amount) {
    var left = amount === 'left', lvl = quotaLevel(w.used), v = left ? 100 - w.used : w.used, p = pace(w, now), mk = left ? 100 - p.expected : p.expected;
    var val = String(Math.round(v)) + '%'; while (val.length < 4) val = ' ' + val;
    var bar = '<span class="pbar"><i style="width:' + clamp(v, 0, 100).toFixed(1) + '%"></i>' + (lvl === 'full' ? '' : '<b style="left:' + mk.toFixed(1) + '%"></b>') + '</span>';
    var cls = 'ca-qchunk ' + style + (lvl ? ' ' + lvl : '');
    if (style === 'percent') return '<span class="' + cls + '"><span class="lbl">' + w.label + '</span><span class="v">' + val + '</span></span>';
    if (style === 'bars') return '<span class="' + cls + '"><span class="lbl">' + w.label + '</span>' + bar + '</span>';
    return '<span class="' + cls + '"><span class="lbl">' + w.label + '</span><span class="v">' + val + '</span>' + bar + '</span>';
  }
  var QUOTA_DEFAULTS = { show: { session: true, weekly: true, fable: true }, resetInPanel: false, style: 'bars', amount: 'used', reset: 'relative', every: '5' };
  /* Whole panel-button content: robot icon, then one chunk per enabled window (+ optional session reset). */
  function quotaPanelHTML(q, now, st) {
    st = st || QUOTA_DEFAULTS;
    var ws = q.windows.filter(function (w) { return st.show[w.key]; });
    return '<span class="ca-icon i-robot ca-qicon" aria-hidden="true"></span><span class="ca-qrow">' + ws.map(function (w) { return quotaChunkHTML(w, now, st.style, st.amount); }).join('') + (st.resetInPanel ? resetChunkHTML(q.windows[0], now) : '') + '</span>';
  }
  function resetChunkHTML(w, now) { var f = formatReset(w.resetAt, now, 'relative', true); while (f.length < 5) f = ' ' + f; return '<span class="ca-qchunk percent reset"><span class="lbl">Reset</span><span class="v">' + f + '</span></span>'; }
  function agoText(ts, now) { var m = Math.round((now - ts) / 60e3); return m < 1 ? 'just now' : m < 60 ? m + 'm ago' : Math.floor(m / 60) + 'h ago'; }

  window.CosmicApplets = {
    HISTORY: HISTORY, formatRate: formatRate, formatCompact: formatCompact, formatBytes: formatBytes,
    niceMax: niceMax, ratesHTML: ratesHTML, CHEV_DOWN: CHEV_DOWN, CHEV_UP: CHEV_UP, INDICATORS: INDICATORS, IND: IND, DEFAULT_INDICATOR: DEFAULT_INDICATOR, pickerHTML: pickerHTML, quotaSim: quotaSim, formatReset: formatReset, resetPhrase: resetPhrase, pace: pace, quotaRowHTML: quotaRowHTML, quotaChunkHTML: quotaChunkHTML, resetChunkHTML: resetChunkHTML, quotaPanelHTML: quotaPanelHTML, QUOTA_DEFAULTS: QUOTA_DEFAULTS, agoText: agoText, formatPct: formatPct, formatTemp: formatTemp, formatGHz: formatGHz, formatW: formatW, formatGiB: formatGiB, sysSim: sysSim, drawSeries: drawSeries, meterHTML: meterHTML, sysSectionHTML: sysSectionHTML, paintSysGraphs: paintSysGraphs, metricChunkHTML: metricChunkHTML, drawGraph: drawGraph, sim: sim
  };
})();
