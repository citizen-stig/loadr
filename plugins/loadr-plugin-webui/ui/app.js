/* loadr app.js — routing, pages and live dashboards. No build step. */
'use strict';

(function () {
  // -------------------------------------------------------------------------
  // Tiny DOM + formatting helpers
  // -------------------------------------------------------------------------
  const $ = (sel, root) => (root || document).querySelector(sel);

  function h(tag, props, ...children) {
    const node = document.createElement(tag);
    if (props) {
      for (const [k, v] of Object.entries(props)) {
        if (v == null) continue;
        if (k === 'class') node.className = v;
        else if (k === 'dataset') Object.assign(node.dataset, v);
        else if (k.startsWith('on') && typeof v === 'function') {
          node.addEventListener(k.slice(2), v);
        } else if (k === 'value') node.value = v;
        else if (k === 'checked') node.checked = !!v;
        else node.setAttribute(k, v);
      }
    }
    for (const child of children.flat(Infinity)) {
      if (child == null) continue;
      node.append(child.nodeType ? child : document.createTextNode(String(child)));
    }
    return node;
  }

  const fmt = {
    num(v, digits) {
      if (v == null || !isFinite(v)) return '–';
      const abs = Math.abs(v);
      if (abs >= 1e9) return (v / 1e9).toFixed(2) + 'B';
      if (abs >= 1e6) return (v / 1e6).toFixed(2) + 'M';
      if (abs >= 1e4) return (v / 1e3).toFixed(1) + 'k';
      if (digits != null) return v.toFixed(digits);
      return abs >= 100 || Number.isInteger(v) ? v.toFixed(0) : v.toFixed(1);
    },
    ms(v) {
      if (v == null || !isFinite(v)) return '–';
      if (v >= 1000) return (v / 1000).toFixed(2) + 's';
      if (v >= 1) return v.toFixed(1) + 'ms';
      return (v * 1000).toFixed(0) + 'µs';
    },
    pct(v) {
      if (v == null || !isFinite(v)) return '–';
      return (v * 100).toFixed(2) + '%';
    },
    bytes(v) {
      if (v == null || !isFinite(v)) return '–';
      const units = ['B', 'KB', 'MB', 'GB', 'TB'];
      let i = 0;
      while (v >= 1024 && i < units.length - 1) {
        v /= 1024;
        i++;
      }
      return v.toFixed(v >= 100 || i === 0 ? 0 : 1) + ' ' + units[i];
    },
    duration(secs) {
      if (secs == null || !isFinite(secs)) return '–';
      const s = Math.max(0, Math.round(secs));
      const hh = Math.floor(s / 3600);
      const mm = Math.floor((s % 3600) / 60);
      const ss = s % 60;
      if (hh) return hh + 'h ' + mm + 'm ' + ss + 's';
      if (mm) return mm + 'm ' + ss + 's';
      return ss + 's';
    },
    time(ms) {
      if (!ms) return '–';
      return new Date(ms).toLocaleTimeString();
    },
    dateTime(ms) {
      if (!ms) return '–';
      return new Date(ms).toLocaleString();
    },
    ago(ms) {
      if (!ms) return '–';
      const d = Math.max(0, Date.now() - ms) / 1000;
      if (d < 5) return 'just now';
      if (d < 60) return Math.floor(d) + 's ago';
      if (d < 3600) return Math.floor(d / 60) + 'm ago';
      if (d < 86400) return Math.floor(d / 3600) + 'h ago';
      return Math.floor(d / 86400) + 'd ago';
    },
  };

  // -------------------------------------------------------------------------
  // Failure breakdown export (CSV / HTML report)
  // -------------------------------------------------------------------------

  // Quote a CSV field per RFC 4180 when it contains a comma, quote, or newline.
  function csvField(value) {
    const s = value == null ? '' : String(value);
    if (/[",\n\r]/.test(s)) return '"' + s.replace(/"/g, '""') + '"';
    return s;
  }

  // Flatten a failures breakdown object into CSV rows: category, cause, count, share.
  function failuresToCsv(f) {
    const lines = ['category,cause,count,share_pct'];
    if (f) {
      const groups = [
        ['http_status', f.by_status],
        ['transport_error', f.by_error_kind],
        ['failed_check', f.by_check],
        ['script_exception', f.by_exception],
      ];
      for (const [category, rows] of groups) {
        for (const r of rows || []) {
          lines.push(
            [
              csvField(category),
              csvField(r.key),
              csvField(r.count),
              csvField(((r.share || 0) * 100).toFixed(2)),
            ].join(',')
          );
        }
      }
    }
    return lines.join('\r\n') + '\r\n';
  }

  // Render a self-contained HTML report of the failure breakdown.
  function failuresToHtml(f, runLabel) {
    const esc = (s) =>
      String(s == null ? '' : s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
    const section = (title, rows) => {
      const body = (rows || []).length
        ? (rows || [])
            .map(
              (r) =>
                '<tr><td>' +
                esc(r.key) +
                '</td><td class="n">' +
                esc(r.count) +
                '</td><td class="n">' +
                ((r.share || 0) * 100).toFixed(1) +
                '%</td></tr>'
            )
            .join('')
        : '<tr><td colspan="3" class="muted">none</td></tr>';
      return (
        '<h2>' +
        esc(title) +
        '</h2><table><thead><tr><th>Cause</th><th class="n">Count</th><th class="n">Share</th></tr></thead><tbody>' +
        body +
        '</tbody></table>'
      );
    };
    const total = (f && f.total) || 0;
    return (
      '<!doctype html><html><head><meta charset="utf-8"><title>loadr failure breakdown</title>' +
      '<style>body{font:14px system-ui,sans-serif;margin:2rem;color:#111;background:#fff}' +
      'h1{margin:0 0 .25rem}h2{margin:1.5rem 0 .5rem;font-size:1rem}' +
      'table{border-collapse:collapse;width:100%;max-width:680px}' +
      'th,td{text-align:left;padding:.35rem .6rem;border-bottom:1px solid #e5e7eb}' +
      '.n{text-align:right;font-variant-numeric:tabular-nums}.muted{color:#888}' +
      '.sub{color:#666;margin:0 0 1rem}</style></head><body>' +
      '<h1>loadr — failure breakdown</h1>' +
      '<p class="sub">' +
      esc(runLabel || '') +
      (runLabel ? ' · ' : '') +
      esc(total) +
      ' total failures · generated ' +
      esc(new Date().toLocaleString()) +
      '</p>' +
      section('HTTP status (4xx / 5xx)', f && f.by_status) +
      section('Transport / error kind', f && f.by_error_kind) +
      section('Failed checks', f && f.by_check) +
      section('Script exceptions', f && f.by_exception) +
      '</body></html>'
    );
  }

  // Trigger a browser download of `content` as a file (no server round-trip).
  function triggerDownload(filename, content, mime) {
    const blob = new Blob([content], { type: mime });
    const url = URL.createObjectURL(blob);
    const a = h('a', { href: url, download: filename });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 0);
  }

  function fileStamp() {
    return new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
  }

  function downloadFailuresCsv(f) {
    if (!f || !f.total) return;
    triggerDownload('loadr-failures-' + fileStamp() + '.csv', failuresToCsv(f), 'text/csv;charset=utf-8');
  }

  function downloadFailuresHtml(f, runLabel) {
    if (!f || !f.total) return;
    triggerDownload('loadr-failures-' + fileStamp() + '.html', failuresToHtml(f, runLabel), 'text/html;charset=utf-8');
  }

  function statePill(state, passed) {
    let cls = state;
    let label = state;
    if (state === 'finished') {
      cls = passed === false ? 'failed' : 'passed';
      label = passed === false ? 'failed' : 'passed';
    }
    return h('span', { class: 'pill pill-' + cls }, label);
  }

  // -------------------------------------------------------------------------
  // Toasts
  // -------------------------------------------------------------------------
  const Toast = {
    show(message, kind) {
      const node = h('div', { class: 'toast toast-' + (kind || 'info') }, message);
      $('#toasts').append(node);
      setTimeout(() => {
        node.classList.add('toast-out');
        setTimeout(() => node.remove(), 300);
      }, 4000);
    },
    error(message) {
      this.show(message, 'error');
    },
    ok(message) {
      this.show(message, 'ok');
    },
  };

  // -------------------------------------------------------------------------
  // Login modal
  // -------------------------------------------------------------------------
  const Login = {
    mode: 'basic',
    visible: false,
    init() {
      $('#login-tab-basic').addEventListener('click', () => this.setMode('basic'));
      $('#login-tab-token').addEventListener('click', () => this.setMode('token'));
      $('#login-form').addEventListener('submit', (e) => {
        e.preventDefault();
        this.submit();
      });
      API.on('unauthorized', () => this.show());
    },
    setMode(mode) {
      this.mode = mode;
      $('#login-tab-basic').classList.toggle('active', mode === 'basic');
      $('#login-tab-token').classList.toggle('active', mode === 'token');
      $('#login-basic-fields').classList.toggle('hidden', mode !== 'basic');
      $('#login-token-fields').classList.toggle('hidden', mode !== 'token');
    },
    show() {
      if (this.visible) return;
      this.visible = true;
      $('#login-modal').classList.remove('hidden');
      ($('#login-user') || {}).focus && $('#login-user').focus();
    },
    hide() {
      this.visible = false;
      $('#login-modal').classList.add('hidden');
    },
    async submit() {
      const errBox = $('#login-error');
      errBox.classList.add('hidden');
      let header;
      if (this.mode === 'token') {
        const token = $('#login-token').value.trim();
        if (!token) return;
        header = 'Bearer ' + token;
      } else {
        const user = $('#login-user').value;
        const pass = $('#login-pass').value;
        header = 'Basic ' + btoa(user + ':' + pass);
      }
      try {
        const res = await fetch('/api/overview', { headers: { Authorization: header } });
        if (res.status === 401) throw new Error('Invalid credentials');
        if (!res.ok) throw new Error('Server error ' + res.status);
        API.setAuth(header);
        this.hide();
        Toast.ok('Signed in');
        App.route(); // re-mount the current page with auth in place
      } catch (e) {
        errBox.textContent = e.message;
        errBox.classList.remove('hidden');
      }
    },
  };

  // -------------------------------------------------------------------------
  // Shared live dashboard (overview + live run detail)
  // -------------------------------------------------------------------------
  const COLORS = {
    blue: '#f87171',
    green: '#4ade80',
    amber: '#fbbf24',
    red: '#ef4444',
    purple: '#fb7185',
    cyan: '#fca5a5',
  };

  function createLiveDash(root) {
    const cards = {
      rps: null,
      vus: null,
      err: null,
      p95: null,
    };
    const cardRow = h(
      'div',
      { class: 'stat-grid' },
      ...Object.entries({
        rps: 'Requests / sec',
        vus: 'Active VUs',
        err: 'Error rate',
        p95: 'p95 latency',
      }).map(([key, label]) => {
        const value = h('div', { class: 'stat-value mono' }, '–');
        cards[key] = value;
        return h('div', { class: 'card stat-card' }, h('div', { class: 'stat-label' }, label), value);
      })
    );

    const rpsCanvas = h('canvas', { class: 'chart' });
    const latCanvas = h('canvas', { class: 'chart' });
    const errCanvas = h('canvas', { class: 'chart chart-short' });

    // Chart-style chooser (line / area / bars), persisted across sessions.
    const CHART_TYPES = [
      ['line', 'Line'],
      ['area', 'Area'],
      ['bars', 'Bars'],
    ];
    let chartType = localStorage.getItem('loadr.chartType') || 'line';
    if (!CHART_TYPES.some(([v]) => v === chartType)) chartType = 'line';
    const typeButtons = CHART_TYPES.map(([val, label]) =>
      h(
        'button',
        {
          class: 'seg-btn' + (val === chartType ? ' active' : ''),
          type: 'button',
          'data-type': val,
        },
        label
      )
    );
    const chartToolbar = h(
      'div',
      { class: 'chart-toolbar' },
      h('span', { class: 'chart-toolbar-label muted' }, 'Chart style'),
      h('div', { class: 'seg' }, ...typeButtons)
    );

    const checksBar = h('div', { class: 'checks-bar-fill' });
    const checksText = h('span', { class: 'mono muted' }, 'no checks yet');
    const scenarioBody = h('tbody');
    const thresholdList = h('div', { class: 'threshold-list' });
    const dataRates = h('div', { class: 'muted mono data-rates' }, '');

    // Failure & error breakdown panel: groups failures by cause and offers a
    // browser-side CSV / HTML download of the breakdown.
    let lastFailures = null;
    const failGroups = {
      by_status: h('div', { class: 'fail-group-body' }),
      by_error_kind: h('div', { class: 'fail-group-body' }),
      by_check: h('div', { class: 'fail-group-body' }),
      by_exception: h('div', { class: 'fail-group-body' }),
    };
    const failSummary = h('span', { class: 'mono muted' }, 'no failures yet');
    const failGroupCard = (key, title) =>
      h('div', { class: 'fail-group' }, h('h4', null, title), failGroups[key]);
    const dlCsvBtn = h(
      'button',
      { class: 'btn btn-ghost btn-sm', type: 'button', title: 'Download breakdown as CSV' },
      '↓ CSV'
    );
    const dlHtmlBtn = h(
      'button',
      { class: 'btn btn-ghost btn-sm', type: 'button', title: 'Download breakdown as an HTML report' },
      '↓ Report'
    );
    dlCsvBtn.addEventListener('click', () => downloadFailuresCsv(lastFailures));
    dlHtmlBtn.addEventListener('click', () => downloadFailuresHtml(lastFailures));
    const failuresCard = h(
      'div',
      { class: 'card fail-card' },
      h(
        'div',
        { class: 'fail-head' },
        h('h3', null, 'Failure breakdown'),
        h('div', { class: 'fail-actions' }, failSummary, dlCsvBtn, dlHtmlBtn)
      ),
      h(
        'div',
        { class: 'fail-grid' },
        failGroupCard('by_status', 'HTTP status'),
        failGroupCard('by_error_kind', 'Transport / error'),
        failGroupCard('by_check', 'Failed checks'),
        failGroupCard('by_exception', 'Script exceptions')
      )
    );

    root.append(
      cardRow,
      chartToolbar,
      h(
        'div',
        { class: 'chart-grid' },
        h('div', { class: 'card' }, h('h3', null, 'Throughput'), rpsCanvas),
        h('div', { class: 'card' }, h('h3', null, 'Latency percentiles'), latCanvas)
      ),
      h(
        'div',
        { class: 'chart-grid' },
        h('div', { class: 'card' }, h('h3', null, 'Error rate'), errCanvas),
        h(
          'div',
          { class: 'card' },
          h('h3', null, 'Checks'),
          h('div', { class: 'checks-bar' }, checksBar),
          h('div', { class: 'checks-meta' }, checksText),
          h('h3', { class: 'mt' }, 'Thresholds'),
          thresholdList
        )
      ),
      h(
        'div',
        { class: 'card' },
        h('h3', null, 'Scenarios'),
        h(
          'table',
          { class: 'table' },
          h(
            'thead',
            null,
            h(
              'tr',
              null,
              h('th', null, 'Scenario'),
              h('th', { class: 'num' }, 'RPS'),
              h('th', { class: 'num' }, 'avg'),
              h('th', { class: 'num' }, 'p95'),
              h('th', { class: 'num' }, 'Errors')
            )
          ),
          scenarioBody
        ),
        dataRates
      ),
      failuresCard
    );

    const rpsChart = new TimeChart(rpsCanvas, {
      series: [{ label: 'req/s', color: COLORS.blue, fill: true }],
      type: chartType,
    });
    const latChart = new TimeChart(latCanvas, {
      series: [
        { label: 'p50', color: COLORS.green },
        { label: 'p90', color: COLORS.cyan },
        { label: 'p95', color: COLORS.amber },
        { label: 'p99', color: COLORS.red },
      ],
      format: (v) => fmt.ms(v),
      type: chartType,
    });
    const errChart = new TimeChart(errCanvas, {
      series: [{ label: 'error %', color: COLORS.red, fill: true }],
      format: (v) => v.toFixed(1) + '%',
      type: chartType,
    });

    // Apply a chart style to every chart, persist it, and reflect the active button.
    function applyChartType(t) {
      chartType = t;
      try {
        localStorage.setItem('loadr.chartType', t);
      } catch (_) {
        /* private mode / storage disabled — non-fatal */
      }
      [rpsChart, latChart, errChart].forEach((c) => c.setType(t));
      typeButtons.forEach((b) => b.classList.toggle('active', b.getAttribute('data-type') === t));
    }
    typeButtons.forEach((b) =>
      b.addEventListener('click', () => applyChartType(b.getAttribute('data-type')))
    );

    function update(m) {
      if (!m) return;
      cards.rps.textContent = fmt.num(m.rps, 1);
      cards.vus.textContent = fmt.num(m.active_vus, 0);
      cards.err.textContent = fmt.pct(m.error_rate == null ? 0 : m.error_rate);
      cards.err.classList.toggle('stat-bad', (m.error_rate || 0) > 0.01);
      cards.p95.textContent = fmt.ms(m.latency && m.latency.p95);

      const t = m.ts || Date.now();
      rpsChart.push(t, [m.rps]);
      const lat = m.latency || {};
      latChart.push(t, [lat.p50, lat.p90, lat.p95, lat.p99]);
      errChart.push(t, [(m.error_rate || 0) * 100]);

      // Checks.
      const checks = m.checks || { passes: 0, fails: 0 };
      const total = checks.passes + checks.fails;
      if (total > 0) {
        const pct = (100 * checks.passes) / total;
        checksBar.style.width = pct + '%';
        checksBar.classList.toggle('bad', pct < 100);
        checksText.textContent =
          pct.toFixed(2) + '% — ✓ ' + fmt.num(checks.passes, 0) + '  ✗ ' + fmt.num(checks.fails, 0);
      } else {
        checksBar.style.width = '0%';
        checksText.textContent = 'no checks yet';
      }

      // Thresholds.
      thresholdList.replaceChildren(
        ...(m.thresholds && m.thresholds.length
          ? m.thresholds.map((th) =>
              h(
                'div',
                { class: 'threshold-row' },
                h('span', { class: 'pill ' + (th.passed ? 'pill-passed' : 'pill-failed') }, th.passed ? 'pass' : 'fail'),
                h('span', { class: 'mono' }, th.metric + ': ' + th.expression),
                h(
                  'span',
                  { class: 'mono muted' },
                  th.observed == null ? 'no samples' : 'observed ' + fmt.num(th.observed, 2)
                )
              )
            )
          : [h('div', { class: 'muted' }, 'No thresholds defined')])
      );

      // Scenario table.
      scenarioBody.replaceChildren(
        ...(m.per_scenario && m.per_scenario.length
          ? m.per_scenario.map((s) =>
              h(
                'tr',
                null,
                h('td', null, s.scenario),
                h('td', { class: 'num mono' }, fmt.num(s.rps, 1)),
                h('td', { class: 'num mono' }, fmt.ms(s.avg)),
                h('td', { class: 'num mono' }, fmt.ms(s.p95)),
                h('td', { class: 'num mono' }, fmt.pct(s.error_rate == null ? 0 : s.error_rate))
              )
            )
          : [h('tr', null, h('td', { colspan: 5, class: 'muted' }, 'no scenario data yet'))])
      );

      dataRates.textContent =
        '↑ ' + fmt.bytes(m.data_sent_ps) + '/s    ↓ ' + fmt.bytes(m.data_received_ps) + '/s    total reqs ' + fmt.num(m.http_reqs_total, 0);

      updateFailures(m.failures);
    }

    // Render the failure breakdown groups and refresh the download buttons.
    function updateFailures(f) {
      lastFailures = f || null;
      const total = (f && f.total) || 0;
      if (total > 0) {
        failSummary.textContent =
          fmt.num(total, 0) +
          ' failures — ' +
          fmt.num(f.failed_requests || 0, 0) +
          ' req · ' +
          fmt.num(f.failed_checks || 0, 0) +
          ' check · ' +
          fmt.num(f.exceptions || 0, 0) +
          ' exc';
        failSummary.classList.remove('muted');
      } else {
        failSummary.textContent = 'no failures yet';
        failSummary.classList.add('muted');
      }
      dlCsvBtn.disabled = total === 0;
      dlHtmlBtn.disabled = total === 0;

      const renderGroup = (key) => {
        const rows = (f && f[key]) || [];
        if (!rows.length) {
          failGroups[key].replaceChildren(h('div', { class: 'muted fail-empty' }, 'none'));
          return;
        }
        failGroups[key].replaceChildren(
          ...rows.map((r) =>
            h(
              'div',
              { class: 'fail-row', title: r.key },
              h('div', { class: 'fail-bar' }, h('div', { class: 'fail-bar-fill', style: 'width:' + Math.max(2, (r.share || 0) * 100).toFixed(1) + '%' })),
              h('span', { class: 'fail-key mono' }, r.key),
              h('span', { class: 'fail-count mono' }, fmt.num(r.count, 0)),
              h('span', { class: 'fail-share mono muted' }, ((r.share || 0) * 100).toFixed(1) + '%')
            )
          )
        );
      };
      renderGroup('by_status');
      renderGroup('by_error_kind');
      renderGroup('by_check');
      renderGroup('by_exception');
    }

    function destroy() {
      rpsChart.destroy();
      latChart.destroy();
      errChart.destroy();
    }

    return { update, destroy };
  }

  // -------------------------------------------------------------------------
  // Pages
  // -------------------------------------------------------------------------
  const Pages = {};

  Pages.overview = {
    mount(main) {
      const runLine = h('div', { class: 'page-sub muted' }, 'Waiting for data…');
      const dashRoot = h('div');
      main.append(h('div', { class: 'page-head' }, h('h1', null, 'Overview'), runLine), dashRoot);

      const dash = createLiveDash(dashRoot);
      const stream = API.sse('/api/stream', {
        overview: (o) => {
          if (o.run) {
            runLine.replaceChildren(
              statePill(o.run.state, o.run.passed),
              h('span', null, ' ' + (o.run.name || o.run.run_id.slice(0, 8)) + ' · '),
              h('a', { href: '#/runs/' + o.run.run_id }, 'view run'),
              h('span', { class: 'muted' }, '  ·  ' + o.live_runs + ' live / ' + o.total_runs + ' total runs')
            );
            dash.update(o.metrics);
          } else {
            runLine.replaceChildren(
              h('span', null, 'No runs yet — start one from the '),
              h('a', { href: '#/tests' }, 'Tests'),
              h('span', null, ' page.')
            );
          }
        },
      });
      return () => {
        stream.close();
        dash.destroy();
      };
    },
  };

  Pages.runs = {
    mount(main) {
      const tbody = h('tbody');
      main.append(
        h('div', { class: 'page-head' }, h('h1', null, 'Runs')),
        h(
          'div',
          { class: 'card' },
          h(
            'table',
            { class: 'table table-click' },
            h(
              'thead',
              null,
              h(
                'tr',
                null,
                h('th', null, 'Status'),
                h('th', null, 'Name'),
                h('th', null, 'Run id'),
                h('th', null, 'Scenarios'),
                h('th', null, 'Started'),
                h('th', { class: 'num' }, 'Duration')
              )
            ),
            tbody
          )
        )
      );

      async function refresh() {
        try {
          const runs = await API.get('/api/runs');
          tbody.replaceChildren(
            ...(runs.length
              ? runs.map((r) => {
                  const dur = r.ended_ms
                    ? (r.ended_ms - r.started_ms) / 1000
                    : (Date.now() - r.started_ms) / 1000;
                  return h(
                    'tr',
                    { onclick: () => (location.hash = '#/runs/' + r.run_id) },
                    h('td', null, statePill(r.state, r.passed)),
                    h('td', null, r.name || h('span', { class: 'muted' }, 'unnamed')),
                    h('td', { class: 'mono muted' }, r.run_id.slice(0, 8)),
                    h('td', { class: 'muted' }, r.scenarios.join(', ')),
                    h('td', null, fmt.dateTime(r.started_ms)),
                    h('td', { class: 'num mono' }, fmt.duration(dur))
                  );
                })
              : [h('tr', null, h('td', { colspan: 6, class: 'muted' }, 'No runs yet.'))])
          );
        } catch (e) {
          if (e.status !== 401) Toast.error('Could not load runs: ' + e.message);
        }
      }
      refresh();
      const timer = setInterval(refresh, 2000);
      return () => clearInterval(timer);
    },
  };

  Pages.runDetail = {
    mount(main, runId) {
      const head = h('div', { class: 'page-head' });
      const controls = h('div', { class: 'run-controls' });
      const body = h('div');
      main.append(head, controls, body);
      let dash = null;
      let stream = null;
      let finishedShown = false;

      function renderHead(run, paused) {
        head.replaceChildren(
          h(
            'h1',
            null,
            h('a', { href: '#/runs', class: 'back-link' }, '←'),
            ' ',
            run.name || 'Run',
            ' ',
            statePill(run.state, run.passed)
          ),
          h(
            'div',
            { class: 'page-sub muted mono' },
            run.run_id + ' · started ' + fmt.dateTime(run.started_ms) + (paused ? ' · PAUSED' : '')
          )
        );
      }

      async function act(label, fn) {
        try {
          await fn();
          Toast.ok(label);
        } catch (e) {
          Toast.error(label + ' failed: ' + e.message);
        }
      }

      function renderControls(detail) {
        const live = ['pending', 'running', 'stopping'].includes(detail.run.state);
        if (!live) {
          controls.replaceChildren();
          return;
        }
        const paused = detail.is_paused;
        const kids = [
          h(
            'button',
            {
              class: 'btn',
              onclick: () => act('Stop requested', () => API.post('/api/runs/' + runId + '/stop', { kill: false })),
            },
            'Stop'
          ),
          h(
            'button',
            {
              class: 'btn btn-danger',
              onclick: () => act('Kill requested', () => API.post('/api/runs/' + runId + '/stop', { kill: true })),
            },
            'Kill'
          ),
          h(
            'button',
            {
              class: 'btn',
              onclick: () =>
                act(paused ? 'Resumed' : 'Paused', () =>
                  API.post('/api/runs/' + runId + '/pause', { paused: !paused }).then(load)
                ),
            },
            paused ? 'Resume' : 'Pause'
          ),
        ];
        for (const scenario of detail.externally_controlled || []) {
          const input = h('input', { class: 'input input-small mono', type: 'number', min: '0', placeholder: 'VUs' });
          kids.push(
            h(
              'span',
              { class: 'scale-group' },
              h('span', { class: 'muted' }, scenario + ':'),
              input,
              h(
                'button',
                {
                  class: 'btn',
                  onclick: () => {
                    const vus = parseInt(input.value, 10);
                    if (isNaN(vus) || vus < 0) {
                      Toast.error('Enter a VU count');
                      return;
                    }
                    act('Scaled ' + scenario + ' to ' + vus, () =>
                      API.post('/api/runs/' + runId + '/scale', { scenario, vus })
                    );
                  },
                },
                'Apply'
              )
            )
          );
        }
        controls.replaceChildren(...kids);
      }

      function showLive(detail) {
        body.replaceChildren();
        const dashRoot = h('div');
        body.append(dashRoot);
        dash = createLiveDash(dashRoot);
        stream = API.sse('/api/runs/' + runId + '/stream', {
          snapshot: (m) => dash.update(m),
          status: (s) => {
            if (['finished', 'failed'].includes(s.state) && !finishedShown) {
              finishedShown = true;
              Toast.ok('Run ' + (s.passed === false ? 'finished: thresholds FAILED' : 'finished'));
              setTimeout(load, 400);
            }
          },
          end: () => {
            if (!finishedShown) {
              finishedShown = true;
              setTimeout(load, 400);
            }
          },
        });
      }

      function metricRow(m) {
        const a = m.agg || {};
        if (m.kind === 'trend') {
          return [
            fmt.num(a.count, 0),
            fmt.ms(a.avg),
            fmt.ms(a.min),
            fmt.ms(a.med),
            fmt.ms(a.p90),
            fmt.ms(a.p95),
            fmt.ms(a.p99),
            fmt.ms(a.max),
          ];
        }
        if (m.kind === 'counter') {
          return [fmt.num(a.sum, 0), fmt.num(a.per_second, 1) + '/s', '', '', '', '', '', ''];
        }
        if (m.kind === 'rate') {
          return [fmt.num(a.count, 0), fmt.pct(a.rate), '', '', '', '', '', ''];
        }
        return [fmt.num(a.last, 1), 'min ' + fmt.num(a.min, 1), 'max ' + fmt.num(a.max, 1), '', '', '', '', ''];
      }

      function showSummary(detail, summary) {
        if (stream) {
          stream.close();
          stream = null;
        }
        if (dash) {
          dash.destroy();
          dash = null;
        }
        body.replaceChildren();
        const cards = h(
          'div',
          { class: 'stat-grid' },
          statCard('Duration', fmt.duration(summary.duration_secs)),
          statCard('Requests', fmt.num(sumMetric(summary, 'http_reqs'), 0)),
          statCard('Avg RPS', fmt.num(perSecMetric(summary, 'http_reqs'), 1)),
          statCard('p95 latency', fmt.ms(aggOf(summary, 'http_req_duration', 'p95')))
        );

        const metricsTable = h(
          'table',
          { class: 'table' },
          h(
            'thead',
            null,
            h(
              'tr',
              null,
              h('th', null, 'Metric'),
              h('th', { class: 'num' }, 'count'),
              h('th', { class: 'num' }, 'avg / rate'),
              h('th', { class: 'num' }, 'min'),
              h('th', { class: 'num' }, 'med'),
              h('th', { class: 'num' }, 'p90'),
              h('th', { class: 'num' }, 'p95'),
              h('th', { class: 'num' }, 'p99'),
              h('th', { class: 'num' }, 'max')
            )
          ),
          h(
            'tbody',
            null,
            summary.metrics.map((m) =>
              h(
                'tr',
                null,
                h('td', { class: 'mono' }, m.metric, ' ', h('span', { class: 'muted' }, m.kind)),
                ...metricRow(m).map((cell) => h('td', { class: 'num mono' }, cell))
              )
            )
          )
        );

        const checks = h(
          'div',
          null,
          summary.checks.length
            ? summary.checks.map((c) =>
                h(
                  'div',
                  { class: 'threshold-row' },
                  h('span', { class: 'pill ' + (c.fails ? 'pill-failed' : 'pill-passed') }, c.fails ? '✗' : '✓'),
                  h('span', null, c.name),
                  h('span', { class: 'mono muted' }, c.passes + ' / ' + (c.passes + c.fails))
                )
              )
            : h('div', { class: 'muted' }, 'No checks in this test')
        );

        const thresholds = h(
          'div',
          null,
          summary.thresholds.length
            ? summary.thresholds.map((t) =>
                h(
                  'div',
                  { class: 'threshold-row' },
                  h('span', { class: 'pill ' + (t.passed ? 'pill-passed' : 'pill-failed') }, t.passed ? 'pass' : 'fail'),
                  h('span', { class: 'mono' }, t.metric + ': ' + t.expression),
                  h(
                    'span',
                    { class: 'mono muted' },
                    t.observed == null ? 'no samples' : 'observed ' + fmt.num(t.observed, 2)
                  )
                )
              )
            : h('div', { class: 'muted' }, 'No thresholds defined')
        );

        body.append(
          summary.aborted ? h('div', { class: 'banner banner-warn' }, 'Run aborted: ' + summary.aborted) : '',
          cards,
          h('div', { class: 'card' }, h('h3', null, 'Metrics'), metricsTable),
          h(
            'div',
            { class: 'chart-grid' },
            h('div', { class: 'card' }, h('h3', null, 'Checks'), checks),
            h('div', { class: 'card' }, h('h3', null, 'Thresholds'), thresholds)
          )
        );
      }

      function statCard(label, value) {
        return h(
          'div',
          { class: 'card stat-card' },
          h('div', { class: 'stat-label' }, label),
          h('div', { class: 'stat-value mono' }, value)
        );
      }
      function metric(summary, name) {
        return (summary.metrics || []).find((m) => m.metric === name);
      }
      function sumMetric(summary, name) {
        const m = metric(summary, name);
        return m ? m.agg.sum : null;
      }
      function perSecMetric(summary, name) {
        const m = metric(summary, name);
        return m ? m.agg.per_second : null;
      }
      function aggOf(summary, name, field) {
        const m = metric(summary, name);
        return m ? m.agg[field] : null;
      }

      async function load() {
        try {
          const detail = await API.get('/api/runs/' + runId);
          renderHead(detail.run, detail.is_paused);
          renderControls(detail);
          const live = ['pending', 'running', 'stopping'].includes(detail.run.state);
          if (live) {
            if (!stream) showLive(detail);
          } else {
            try {
              const summary = await API.get('/api/runs/' + runId + '/summary');
              showSummary(detail, summary);
            } catch (e) {
              body.replaceChildren(
                h('div', { class: 'card muted' }, 'No summary available for this run (' + e.message + ').')
              );
            }
          }
        } catch (e) {
          head.replaceChildren(h('h1', null, 'Run not found'));
          body.replaceChildren(h('div', { class: 'card muted' }, e.message));
        }
      }
      load();
      return () => {
        if (stream) stream.close();
        if (dash) dash.destroy();
      };
    },
  };

  Pages.tests = {
    mount(main) {
      let tests = [];
      let current = null; // name of the loaded test
      const list = h('div', { class: 'test-list' });
      const nameInput = h('input', { class: 'input', placeholder: 'test-name', spellcheck: 'false' });
      const envInput = h('input', { class: 'input input-small', placeholder: 'env (optional)', spellcheck: 'false' });
      const gutter = h('div', { class: 'editor-gutter mono', 'aria-hidden': 'true' }, '1');
      const textarea = h('textarea', {
        class: 'editor-text mono',
        spellcheck: 'false',
        placeholder: '# loadr test definition\nname: my-test\nscenarios:\n  ...',
      });
      const diagBox = h('div', { class: 'diagnostics' });

      const DEFAULT_YAML = [
        'name: new-test',
        'scenarios:',
        '  default:',
        '    executor: constant-vus',
        '    vus: 5',
        '    duration: 30s',
        '    flow:',
        '      - request:',
        '          url: https://example.com/',
        'thresholds:',
        '  http_req_duration:',
        '    - p(95) < 500',
        '',
      ].join('\n');

      function syncGutter() {
        const lines = textarea.value.split('\n').length;
        gutter.replaceChildren(...Array.from({ length: lines }, (_, i) => h('div', null, i + 1)));
      }
      textarea.addEventListener('input', syncGutter);
      textarea.addEventListener('scroll', () => {
        gutter.scrollTop = textarea.scrollTop;
      });

      function renderDiagnostics(diags) {
        if (!diags.length) {
          diagBox.replaceChildren(h('div', { class: 'diag diag-ok' }, '✓ No problems found'));
          return;
        }
        diagBox.replaceChildren(
          ...diags.map((d) =>
            h(
              'div',
              {
                class: 'diag diag-' + d.severity,
                onclick: () => {
                  if (d.line) jumpToLine(d.line);
                },
              },
              h('span', { class: 'diag-sev' }, d.severity),
              h(
                'span',
                { class: 'mono muted' },
                (d.line ? 'line ' + d.line + (d.column ? ':' + d.column : '') : '') + (d.path ? ' (' + d.path + ')' : '')
              ),
              h('span', null, ' ' + d.message + (d.suggestion ? ' — ' + d.suggestion : ''))
            )
          )
        );
      }

      function jumpToLine(line) {
        const lines = textarea.value.split('\n');
        let pos = 0;
        for (let i = 0; i < Math.min(line - 1, lines.length); i++) pos += lines[i].length + 1;
        textarea.focus();
        textarea.setSelectionRange(pos, pos + (lines[line - 1] || '').length);
      }

      function loadTest(t) {
        current = t ? t.name : null;
        nameInput.value = t ? t.name : '';
        textarea.value = t ? t.yaml : DEFAULT_YAML;
        diagBox.replaceChildren();
        syncGutter();
        renderList();
      }

      function renderList() {
        list.replaceChildren(
          h(
            'button',
            { class: 'btn btn-primary btn-block', onclick: () => loadTest(null) },
            '+ New test'
          ),
          ...(tests.length
            ? tests.map((t) =>
                h(
                  'div',
                  {
                    class: 'test-item' + (t.name === current ? ' active' : ''),
                    onclick: () => loadTest(t),
                  },
                  h('div', { class: 'test-item-name' }, t.name),
                  h('div', { class: 'muted test-item-meta' }, 'updated ' + fmt.ago(t.updated_ms))
                )
              )
            : [h('div', { class: 'muted test-empty' }, 'No saved tests yet.')])
        );
      }

      async function refresh() {
        try {
          tests = await API.get('/api/tests');
          renderList();
        } catch (e) {
          if (e.status !== 401) Toast.error('Could not load tests: ' + e.message);
        }
      }

      async function validate() {
        try {
          const res = await API.post('/api/validate', { yaml: textarea.value, env: envInput.value || null });
          renderDiagnostics(res.diagnostics || []);
        } catch (e) {
          Toast.error('Validate failed: ' + e.message);
        }
      }

      async function save() {
        const name = nameInput.value.trim();
        if (!name) {
          Toast.error('Give the test a name first');
          nameInput.focus();
          return;
        }
        try {
          await API.put('/api/tests/' + encodeURIComponent(name), { yaml: textarea.value });
          current = name;
          Toast.ok('Saved `' + name + '`');
          refresh();
        } catch (e) {
          Toast.error('Save failed: ' + e.message);
        }
      }

      async function run() {
        try {
          const res = await API.post('/api/runs', {
            name: nameInput.value.trim() || null,
            yaml: textarea.value,
            env: envInput.value.trim() || null,
          });
          Toast.ok('Run started');
          location.hash = '#/runs/' + res.run_id;
        } catch (e) {
          if (e.body && e.body.diagnostics) {
            renderDiagnostics(e.body.diagnostics);
            Toast.error('Test definition has errors');
          } else {
            Toast.error('Run failed: ' + e.message);
          }
        }
      }

      async function remove() {
        const name = current || nameInput.value.trim();
        if (!name) return;
        try {
          await API.del('/api/tests/' + encodeURIComponent(name));
          Toast.ok('Deleted `' + name + '`');
          loadTest(null);
          refresh();
        } catch (e) {
          Toast.error('Delete failed: ' + e.message);
        }
      }

      main.append(
        h('div', { class: 'page-head' }, h('h1', null, 'Tests')),
        h(
          'div',
          { class: 'tests-layout' },
          h('div', { class: 'card tests-side' }, list),
          h(
            'div',
            { class: 'card tests-editor' },
            h(
              'div',
              { class: 'editor-toolbar' },
              nameInput,
              envInput,
              h('button', { class: 'btn', onclick: validate }, 'Validate'),
              h('button', { class: 'btn btn-primary', onclick: save }, 'Save'),
              h('button', { class: 'btn btn-ok', onclick: run }, '▶ Run'),
              h('button', { class: 'btn btn-danger', onclick: remove }, 'Delete')
            ),
            h('div', { class: 'editor' }, gutter, textarea),
            diagBox
          )
        )
      );
      loadTest(null);
      refresh();
      return () => {};
    },
  };

  Pages.agents = {
    mount(main) {
      const grid = h('div', { class: 'agent-grid' });
      main.append(h('div', { class: 'page-head' }, h('h1', null, 'Agents')), grid);

      async function refresh() {
        try {
          const agents = await API.get('/api/agents');
          if (!agents.length) {
            grid.replaceChildren(
              h(
                'div',
                { class: 'card empty-state' },
                h('div', { class: 'empty-icon' }, '⬡'),
                h('h3', null, 'No agents connected'),
                h('div', { class: 'muted' }, 'Standalone mode: load is generated by this process. Agents appear here in distributed mode.')
              )
            );
            return;
          }
          grid.replaceChildren(
            ...agents.map((a) =>
              h(
                'div',
                { class: 'card agent-card' },
                h(
                  'div',
                  { class: 'agent-head' },
                  h('span', { class: 'health-dot ' + (a.healthy ? 'ok' : 'bad') }),
                  h('strong', null, a.name),
                  h('span', { class: 'muted mono' }, a.id.slice(0, 8))
                ),
                h(
                  'div',
                  { class: 'agent-stats mono' },
                  h('span', null, fmt.num(a.active_vus, 0) + ' VUs'),
                  h('span', null, a.cores + ' cores'),
                  h('span', { class: 'muted' }, 'seen ' + fmt.ago(a.last_heartbeat_ms))
                ),
                h(
                  'div',
                  { class: 'agent-labels' },
                  Object.entries(a.labels || {}).map(([k, v]) => h('span', { class: 'label-chip mono' }, k + '=' + v))
                )
              )
            )
          );
        } catch (e) {
          if (e.status !== 401) Toast.error('Could not load agents: ' + e.message);
        }
      }
      refresh();
      const timer = setInterval(refresh, 2000);
      return () => clearInterval(timer);
    },
  };

  Pages.logs = {
    mount(main) {
      let paused = false;
      const pauseBtn = h(
        'button',
        {
          class: 'btn',
          onclick: () => {
            paused = !paused;
            pauseBtn.textContent = paused ? '▶ Resume' : '⏸ Pause';
          },
        },
        '⏸ Pause'
      );
      const box = h('div', { class: 'log-box mono' });
      main.append(
        h('div', { class: 'page-head' }, h('h1', null, 'Logs'), h('div', { class: 'page-actions' }, pauseBtn)),
        h('div', { class: 'card log-card' }, box)
      );

      async function refresh() {
        if (paused) return;
        try {
          const logs = await API.get('/api/logs');
          const atBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 40;
          box.replaceChildren(
            ...(logs.length
              ? logs.map((l) =>
                  h(
                    'div',
                    { class: 'log-line' },
                    h('span', { class: 'muted' }, fmt.time(l.ts_ms)),
                    h('span', { class: 'log-level log-' + l.level }, l.level.toUpperCase().padEnd(5)),
                    h('span', null, l.message)
                  )
                )
              : [h('div', { class: 'muted' }, 'No log lines yet.')])
          );
          if (atBottom) box.scrollTop = box.scrollHeight;
        } catch (e) {
          if (e.status !== 401) {
            /* transient — keep polling */
          }
        }
      }
      refresh();
      const timer = setInterval(refresh, 2000);
      return () => clearInterval(timer);
    },
  };

  // -------------------------------------------------------------------------
  // Router + theme
  // -------------------------------------------------------------------------
  const App = {
    cleanup: null,
    route() {
      const main = $('#main');
      if (this.cleanup) {
        try {
          this.cleanup();
        } catch (e) {
          /* page teardown should never break navigation */
        }
        this.cleanup = null;
      }
      main.replaceChildren();

      const hash = location.hash || '#/overview';
      const parts = hash.replace(/^#\//, '').split('/');
      let route = parts[0] || 'overview';
      let cleanup;
      if (route === 'runs' && parts[1]) {
        cleanup = Pages.runDetail.mount(main, decodeURIComponent(parts[1]));
        route = 'runs';
      } else if (Pages[route]) {
        cleanup = Pages[route].mount(main);
      } else {
        route = 'overview';
        cleanup = Pages.overview.mount(main);
      }
      this.cleanup = cleanup;

      document.querySelectorAll('#nav a').forEach((a) => {
        a.classList.toggle('active', a.dataset.route === route);
      });
    },
  };
  window.App = App;

  function initTheme() {
    const saved = localStorage.getItem('loadr.theme');
    if (saved) document.documentElement.dataset.theme = saved;
    $('#theme-toggle').addEventListener('click', () => {
      const cur = document.documentElement.dataset.theme === 'light' ? 'dark' : 'light';
      document.documentElement.dataset.theme = cur;
      localStorage.setItem('loadr.theme', cur);
    });
  }

  window.addEventListener('hashchange', () => App.route());
  document.addEventListener('DOMContentLoaded', () => {
    initTheme();
    Login.init();
    App.route();
  });
})();
