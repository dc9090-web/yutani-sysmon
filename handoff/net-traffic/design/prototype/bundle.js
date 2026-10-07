/* @ds-bundle: {"format":4,"namespace":"CosmicApplets","components":[{"name":"PanelButton"},{"name":"RateReadout"},{"name":"TrafficGraph"},{"name":"MenuRow"},{"name":"AdapterList"},{"name":"DisplayModeControl"}]} */
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

  window.CosmicApplets = {
    HISTORY: HISTORY, formatRate: formatRate, formatCompact: formatCompact, formatBytes: formatBytes,
    niceMax: niceMax, ratesHTML: ratesHTML, CHEV_DOWN: CHEV_DOWN, CHEV_UP: CHEV_UP, INDICATORS: INDICATORS, IND: IND, DEFAULT_INDICATOR: DEFAULT_INDICATOR, pickerHTML: pickerHTML, drawGraph: drawGraph, sim: sim
  };
})();
