/* =========================================================================
   MERIDIAN Design System v3 — поведінка
   ========================================================================= */
(function () {
  'use strict';

  var root = document.documentElement;

  /* -----------------------------------------------------------------------
     1. ТЕМА
     ----------------------------------------------------------------------- */
  var theme = 'light';
  var themeSwitch = document.getElementById('themeSwitch');

  function setTheme(next) {
    theme = next;
    root.setAttribute('data-theme', next);
    themeSwitch.querySelectorAll('[data-theme-btn]').forEach(function (b) {
      b.setAttribute('aria-pressed', String(b.dataset.themeBtn === next));
    });
    applyAccent(currentAccent);
  }
  themeSwitch.addEventListener('click', function (e) {
    var b = e.target.closest('[data-theme-btn]');
    if (b) setTheme(b.dataset.themeBtn);
  });

  /* -----------------------------------------------------------------------
     2. АКЦЕНТ
     ----------------------------------------------------------------------- */
  var ACCENTS = [
    { id: 'blue',     name: 'Азурит',   ramp: ['#d9e6fb', '#a7c8f6', '#5b9bec', '#2f6fd0', '#1f56a8', '#163e79'] },
    { id: 'teal',     name: 'Малахіт',  ramp: ['#d3ebe7', '#9ad4cd', '#46b3a6', '#128b7e', '#0d6a61', '#084b45'] },
    { id: 'green',    name: 'Хвоя',     ramp: ['#dcefd9', '#b0dbaa', '#6cbb62', '#3f9639', '#2d7229', '#1e4f1c'] },
    { id: 'amber',    name: 'Бурштин',  ramp: ['#faecd2', '#f2d296', '#e0a83f', '#b57e10', '#8d600b', '#63430a'] },
    { id: 'orange',   name: 'Теракота', ramp: ['#fce4d6', '#f6bf9c', '#ec8b4f', '#cf6524', '#a44b17', '#74340f'] },
    { id: 'red',      name: 'Цинобра',  ramp: ['#fbe3e0', '#f3b8b1', '#e3736a', '#c8483c', '#9f3529', '#71241b'] },
    { id: 'purple',   name: 'Аметист',  ramp: ['#eae0f8', '#cdb4ee', '#a077d8', '#7a4dbb', '#5d3893', '#402566'] },
    { id: 'graphite', name: 'Графіт',   ramp: ['#e5e2db', '#c6c0b5', '#918a7d', '#6a6459', '#4c4740', '#35312c'] }
  ];

  var swatchHost = document.getElementById('swatches');
  var accentName = document.getElementById('accentName');
  var currentAccent = ACCENTS[0];

  function rgb(h) {
    var n = parseInt(h.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255].join(',');
  }

  function applyAccent(acc) {
    currentAccent = acc;
    var s = root.style;
    acc.ramp.forEach(function (c, i) { s.setProperty('--ac-' + (i + 1), c); });

    if (theme === 'dark') {
      s.setProperty('--acc-bg', acc.ramp[3]);
      s.setProperty('--acc-bg-hover', acc.ramp[2]);
      s.setProperty('--acc-fg', '#ffffff');
      s.setProperty('--acc-text', acc.ramp[2]);
      s.setProperty('--acc-soft', 'rgba(' + rgb(acc.ramp[2]) + ',.20)');
      s.setProperty('--acc-soft-fg', acc.ramp[1]);
      s.setProperty('--acc-ring', 'rgba(' + rgb(acc.ramp[2]) + ',.40)');
    } else {
      s.setProperty('--acc-bg', acc.ramp[3]);
      s.setProperty('--acc-bg-hover', acc.ramp[4]);
      s.setProperty('--acc-fg', '#ffffff');
      s.setProperty('--acc-text', acc.ramp[4]);
      s.setProperty('--acc-soft', acc.ramp[0]);
      s.setProperty('--acc-soft-fg', acc.ramp[5]);
      s.setProperty('--acc-ring', 'rgba(' + rgb(acc.ramp[3]) + ',.32)');
    }
    accentName.textContent = acc.name + ' · ' + acc.ramp[3].toUpperCase();
    Array.prototype.forEach.call(swatchHost.children, function (b) {
      b.setAttribute('aria-pressed', String(b.dataset.id === acc.id));
    });
  }

  ACCENTS.forEach(function (acc) {
    var b = document.createElement('button');
    b.className = 'swatch';
    b.dataset.id = acc.id;
    b.title = acc.name;
    b.setAttribute('aria-label', 'Акцент ' + acc.name);
    b.setAttribute('aria-pressed', 'false');
    b.style.background = 'linear-gradient(140deg,' + acc.ramp[2] + ',' + acc.ramp[4] + ')';
    b.addEventListener('click', function () { applyAccent(acc); });
    swatchHost.appendChild(b);
  });
  applyAccent(ACCENTS[0]);

  /* -----------------------------------------------------------------------
     3. СЕГМЕНТОВАНИЙ КОНТРОЛ — рухома підкладка
     ----------------------------------------------------------------------- */
  function placeThumb(group, btn) {
    var thumb = group.querySelector('.seg-thumb');
    if (!thumb || !btn) return;
    thumb.style.width = btn.offsetWidth + 'px';
    thumb.style.transform = 'translateX(' + (btn.offsetLeft - 3) + 'px)';
  }
  var segGroups = Array.prototype.slice.call(document.querySelectorAll('[data-segmented]'));
  segGroups.forEach(function (g) {
    g.addEventListener('click', function (e) {
      var b = e.target.closest('button');
      if (!b) return;
      g.querySelectorAll('button').forEach(function (x) { x.setAttribute('aria-pressed', String(x === b)); });
      placeThumb(g, b);
    });
  });

  /* -----------------------------------------------------------------------
     4. ВКЛАДКИ — рухомий індикатор
     ----------------------------------------------------------------------- */
  var TAB_TEXT = [
    'Загальні параметри робочого середовища Meridian OS.',
    'Ethernet підключено на 1 Гбіт/с. Wi-Fi модуль активний, 4 збережені мережі.',
    'Профіль «Збалансований». Прогнозований час автономності — 6 год 20 хв.',
    'Meridian OS 3.0 «Cassini» · ядро 6.9.2 · оболонка meridian-shell.'
  ];
  var tabs = document.getElementById('m-tabs');
  var tabPanel = document.getElementById('m-tabpanel');

  function placeTabInd(bar, btn) {
    var ind = bar.querySelector('.tab-ind');
    if (!ind || !btn) return;
    ind.style.width = (btn.offsetWidth - 16) + 'px';
    ind.style.transform = 'translateX(' + (btn.offsetLeft + 8) + 'px)';
  }
  if (tabs) {
    tabs.addEventListener('click', function (e) {
      var b = e.target.closest('button');
      if (!b) return;
      var list = Array.prototype.slice.call(tabs.querySelectorAll('button'));
      list.forEach(function (x) { x.setAttribute('aria-selected', String(x === b)); });
      tabPanel.textContent = TAB_TEXT[list.indexOf(b)] || '';
      tabPanel.style.animation = 'none';
      void tabPanel.offsetWidth;
      tabPanel.style.animation = '';
      placeTabInd(tabs, b);
    });
  }

  function realign() {
    segGroups.forEach(function (g) { placeThumb(g, g.querySelector('[aria-pressed="true"]')); });
    if (tabs) placeTabInd(tabs, tabs.querySelector('[aria-selected="true"]'));
  }
  requestAnimationFrame(realign);
  window.addEventListener('resize', realign);
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(realign);

  /* -----------------------------------------------------------------------
     5. РЕШТА КОМПОНЕНТІВ
     ----------------------------------------------------------------------- */
  var pager = document.getElementById('m-pager');
  if (pager) pager.addEventListener('click', function (e) {
    var b = e.target.closest('button');
    if (!b || !/^\d+$/.test(b.textContent.trim())) return;
    pager.querySelectorAll('button').forEach(function (x) { x.removeAttribute('aria-current'); });
    b.setAttribute('aria-current', 'page');
  });

  var indet = document.getElementById('m-indet');
  if (indet) indet.indeterminate = true;

  var clearBtn = document.getElementById('m-clear');
  if (clearBtn) clearBtn.addEventListener('click', function () {
    var i = document.getElementById('m-search'); i.value = ''; i.focus();
  });

  var allBox = document.getElementById('m-all');
  if (allBox) allBox.addEventListener('change', function () {
    document.querySelectorAll('#m-table tbody input[type=checkbox]').forEach(function (c) { c.checked = allBox.checked; });
  });

  (function () {
    var prog = document.getElementById('m-prog'), pct = document.getElementById('m-pct');
    if (!prog) return;
    var v = 62, d = 1;
    setInterval(function () {
      v += d * (2 + Math.random() * 3);
      if (v >= 99) { v = 99; d = -1; }
      if (v <= 18) { v = 18; d = 1; }
      prog.style.width = v.toFixed(0) + '%';
      pct.textContent = v.toFixed(0) + '%';
    }, 900);
  })();

  document.querySelectorAll('.dropdown').forEach(function (dd) {
    var t = dd.querySelector('[data-dd-trigger]');
    var m = dd.querySelector('[data-dd-menu]');
    t.addEventListener('click', function (e) {
      e.stopPropagation();
      var open = !m.classList.contains('open');
      m.classList.toggle('open', open);
      dd.dataset.open = open ? '1' : '0';
    });
    m.addEventListener('click', function () { m.classList.remove('open'); dd.dataset.open = '0'; });
  });
  document.addEventListener('click', function () {
    document.querySelectorAll('[data-dd-menu].open').forEach(function (m) {
      m.classList.remove('open');
      if (m.parentElement) m.parentElement.dataset.open = '0';
    });
  });

  function refreshSlider(wrap) {
    var input = wrap.querySelector('input[type=range]');
    var fill = wrap.querySelector('.fill');
    var min = parseFloat(input.min || 0), max = parseFloat(input.max || 100);
    var r = (parseFloat(input.value) - min) / (max - min);
    /* ручка 18px: заповнення доводиться рівно до її центру, а не до краю доріжки */
    if (fill) {
      var off = 9 - r * 18;
      fill.style.width = 'calc(' + (r * 100).toFixed(2) + '% ' +
                         (off < 0 ? '- ' : '+ ') + Math.abs(off).toFixed(1) + 'px)';
    }
    var host = wrap.parentElement;
    var out = host ? host.querySelector('[data-slider-out]') : null;
    if (out) out.textContent = input.value;
  }
  document.querySelectorAll('[data-slider]').forEach(function (w) {
    refreshSlider(w);
    w.querySelector('input[type=range]').addEventListener('input', function () { refreshSlider(w); });
  });

  document.querySelectorAll('.acc-head').forEach(function (h) {
    h.addEventListener('click', function () { h.parentElement.classList.toggle('open'); });
  });

  /* -----------------------------------------------------------------------
     6. COLOR PICKER
     ----------------------------------------------------------------------- */
  (function () {
    var host = document.getElementById('m-cp');
    if (!host) return;
    var canvas = host.querySelector('[data-cp-canvas]');
    var knob = host.querySelector('[data-cp-knob]');
    var hue = host.querySelector('[data-cp-hue]');
    var hueK = host.querySelector('[data-cp-huek]');
    var preview = host.querySelector('[data-cp-preview]');
    var hexOut = host.querySelector('[data-cp-hex]');
    var hslOut = host.querySelector('[data-cp-hsl]');
    var st = { h: 212, s: .78, v: .74 };

    function hsv2rgb(h, s, v) {
      var c = v * s, x = c * (1 - Math.abs(((h / 60) % 2) - 1)), m = v - c, r = 0, g = 0, b = 0;
      if (h < 60) { r = c; g = x; } else if (h < 120) { r = x; g = c; }
      else if (h < 180) { g = c; b = x; } else if (h < 240) { g = x; b = c; }
      else if (h < 300) { r = x; b = c; } else { r = c; b = x; }
      return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255)];
    }
    function render() {
      canvas.style.setProperty('--cp-hue', st.h);
      knob.style.left = (st.s * 100) + '%';
      knob.style.top = ((1 - st.v) * 100) + '%';
      hueK.style.top = (st.h / 360 * 100) + '%';
      var c = hsv2rgb(st.h, st.s, st.v);
      var hx = '#' + c.map(function (n) { return ('0' + n.toString(16)).slice(-2); }).join('').toUpperCase();
      preview.style.background = hx;
      hexOut.textContent = hx;
      hslOut.textContent = Math.round(st.h) + ' ' + Math.round(st.s * 100) + '% ' +
                           Math.round(st.v * (1 - st.s / 2) * 100) + '%';
    }
    function drag(el, cb) {
      function mv(ev) {
        var r = el.getBoundingClientRect();
        cb(Math.min(1, Math.max(0, (ev.clientX - r.left) / r.width)),
           Math.min(1, Math.max(0, (ev.clientY - r.top) / r.height)));
      }
      el.addEventListener('pointerdown', function (ev) {
        el.setPointerCapture(ev.pointerId); mv(ev);
        function up() { el.removeEventListener('pointermove', mv); document.removeEventListener('pointerup', up); }
        el.addEventListener('pointermove', mv);
        document.addEventListener('pointerup', up);
      });
    }
    drag(canvas, function (x, y) { st.s = x; st.v = 1 - y; render(); });
    drag(hue, function (x, y) { st.h = y * 360; render(); });
    render();
  })();

  /* -----------------------------------------------------------------------
     7. DATE PICKER
     ----------------------------------------------------------------------- */
  (function () {
    var host = document.getElementById('m-dp');
    if (!host) return;
    var MONTHS = ['Січень', 'Лютий', 'Березень', 'Квітень', 'Травень', 'Червень',
                  'Липень', 'Серпень', 'Вересень', 'Жовтень', 'Листопад', 'Грудень'];
    var DOW = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Нд'];
    var grid = host.querySelector('[data-dp-grid]');
    var title = host.querySelector('[data-dp-title]');
    var out = host.querySelector('[data-dp-out]');
    var today = new Date();
    var view = new Date(today.getFullYear(), today.getMonth(), 1);
    var sel = null;
    function fmt(d) { return ('0' + d.getDate()).slice(-2) + '.' + ('0' + (d.getMonth() + 1)).slice(-2) + '.' + d.getFullYear(); }
    function build() {
      grid.innerHTML = '';
      title.textContent = MONTHS[view.getMonth()] + ' ' + view.getFullYear();
      DOW.forEach(function (d) { var s = document.createElement('span'); s.className = 'dow'; s.textContent = d; grid.appendChild(s); });
      var first = new Date(view.getFullYear(), view.getMonth(), 1);
      var off = (first.getDay() + 6) % 7;
      var start = new Date(first); start.setDate(1 - off);
      for (var i = 0; i < 42; i++) {
        var d = new Date(start); d.setDate(start.getDate() + i);
        var b = document.createElement('button');
        b.textContent = d.getDate();
        if (d.getMonth() !== view.getMonth()) b.classList.add('out');
        if (d.toDateString() === today.toDateString()) b.classList.add('today');
        if (sel && d.toDateString() === sel.toDateString()) b.classList.add('sel');
        (function (dd) { b.addEventListener('click', function () { sel = dd; out.textContent = fmt(dd); build(); }); })(d);
        grid.appendChild(b);
      }
    }
    host.querySelector('[data-dp-prev]').addEventListener('click', function () { view.setMonth(view.getMonth() - 1); build(); });
    host.querySelector('[data-dp-next]').addEventListener('click', function () { view.setMonth(view.getMonth() + 1); build(); });
    host.querySelector('[data-dp-today]').addEventListener('click', function () {
      view = new Date(today.getFullYear(), today.getMonth(), 1);
      sel = new Date(today); out.textContent = fmt(today); build();
    });
    build();
  })();

  /* -----------------------------------------------------------------------
     8. СОРТУВАННЯ ТАБЛИЦІ + ГОДИННИК
     ----------------------------------------------------------------------- */
  (function () {
    var table = document.getElementById('m-table');
    if (!table) return;
    var dir = {};
    table.querySelectorAll('th.sortable').forEach(function (th) {
      th.addEventListener('click', function () {
        var idx = parseInt(th.dataset.sort, 10) + 1;
        var asc = !(dir[idx] = !dir[idx]);
        var tb = table.querySelector('tbody');
        var rows = Array.prototype.slice.call(tb.rows);
        rows.sort(function (a, b) {
          var x = a.cells[idx].textContent.trim(), y = b.cells[idx].textContent.trim();
          var nx = parseFloat(x.replace(/[^\d,.-]/g, '').replace(',', '.'));
          var ny = parseFloat(y.replace(/[^\d,.-]/g, '').replace(',', '.'));
          if (!isNaN(nx) && !isNaN(ny)) return asc ? nx - ny : ny - nx;
          return asc ? x.localeCompare(y, 'uk') : y.localeCompare(x, 'uk');
        });
        rows.forEach(function (r) { tb.appendChild(r); });
        table.querySelectorAll('th .arrow').forEach(function (a) { a.remove(); });
        var arrow = document.createElement('span');
        arrow.className = 'arrow';
        arrow.textContent = asc ? '↓' : '↑';
        th.appendChild(arrow);
      });
    });
  })();

  (function () {
    var el = document.getElementById('clock');
    if (!el) return;
    function tick() {
      var d = new Date();
      el.textContent = ('0' + d.getHours()).slice(-2) + ':' + ('0' + d.getMinutes()).slice(-2);
    }
    tick(); setInterval(tick, 15000);
  })();

  /* =======================================================================
     9. МЕНЕДЖЕР ВІКОН
     ======================================================================= */
  var screen = document.getElementById('screen');
  if (!screen) return;

  var TOPBAR = 34;   /* висота системної смуги */
  var DOCKZONE = 66; /* зона дока внизу */

  var wins = Array.prototype.slice.call(screen.querySelectorAll('.dwin'));
  var dash = document.getElementById('dash');
  var ovBtn = document.getElementById('ovBtn');
  var ctxMenu = document.getElementById('ctxMenu');
  var toastSlot = document.getElementById('toastSlot');
  var toastText = document.getElementById('toastText');
  var appNameBtn = document.getElementById('appNameBtn');
  var wall = screen.querySelector('.wall');

  var overview = false;
  var zTop = 12;
  var toastTimer = null;
  var ctxTarget = null;

  var state = {};
  wins.forEach(function (w) {
    state[w.id] = { open: !w.classList.contains('hidden'), minimized: false, maximized: false, saved: null };
  });

  function toast(msg) {
    toastText.textContent = msg;
    toastSlot.classList.add('show');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () { toastSlot.classList.remove('show'); }, 2200);
  }

  function rectIn(el) {
    var a = el.getBoundingClientRect(), b = screen.getBoundingClientRect();
    return { x: a.left - b.left, y: a.top - b.top, w: a.width, h: a.height };
  }

  function dashItemFor(id) { return dash.querySelector('[data-launch="' + id + '"]'); }
  function title(w) { return w.querySelector('.title').textContent; }

  function syncDash() {
    wins.forEach(function (w) {
      var item = dashItemFor(w.id);
      if (!item) return;
      var st = state[w.id];
      item.classList.toggle('running', st.open);
      item.classList.toggle('focused', st.open && !st.minimized && w.classList.contains('active'));
    });
  }

  function focusWindow(w) {
    if (!state[w.id].open || state[w.id].minimized) return;
    wins.forEach(function (x) { x.classList.remove('active'); });
    w.classList.add('active');
    zTop += 1;
    w.style.zIndex = zTop;
    appNameBtn.textContent = title(w);
    syncDash();
  }

  function anim(w, cls, ms, done) {
    w.classList.add(cls);
    setTimeout(function () { w.classList.remove(cls); if (done) done(); }, ms);
  }

  function flightTransform(w, item) {
    var ir = rectIn(item);
    var k = 0.06;
    var dx = (ir.x + ir.w / 2) - w.offsetLeft - (w.offsetWidth * k) / 2;
    var dy = (ir.y + ir.h / 2) - w.offsetTop - (w.offsetHeight * k) / 2;
    return 'translate(' + dx.toFixed(1) + 'px,' + dy.toFixed(1) + 'px) scale(' + k + ')';
  }

  function appear(w, item, msg) {
    w.classList.remove('hidden');
    w.style.transform = item ? flightTransform(w, item) : 'scale(.86)';
    w.style.opacity = '0';
    void w.offsetWidth;
    anim(w, 'anim-tf', 440, function () { w.style.transform = ''; });
    w.style.transform = 'none';
    w.style.opacity = '1';
    focusWindow(w);
    syncDash();
    if (msg) toast(msg);
  }

  function launch(w) {
    var st = state[w.id];
    if (st.open && !st.minimized) { focusWindow(w); return; }
    st.open = true; st.minimized = false;
    appear(w, dashItemFor(w.id), 'Запущено: ' + title(w));
  }

  function minimize(w) {
    var st = state[w.id];
    if (!st.open || st.minimized) return;
    var item = dashItemFor(w.id);
    st.minimized = true;
    w.classList.remove('active');
    anim(w, 'anim-tf', 440, function () {
      if (state[w.id].minimized) { w.classList.add('hidden'); w.style.transform = ''; w.style.opacity = ''; }
    });
    w.style.transform = item ? flightTransform(w, item) : 'scale(.1)';
    w.style.opacity = '0';
    syncDash();
    var next = wins.filter(function (x) { return state[x.id].open && !state[x.id].minimized; }).pop();
    if (next) focusWindow(next); else appNameBtn.textContent = 'Meridian';
    toast('Згорнуто: ' + title(w));
  }

  function closeWin(w) {
    var st = state[w.id];
    if (!st.open) return;
    st.open = false; st.minimized = false;
    w.classList.remove('active');
    anim(w, 'anim-fast', 220, function () {
      w.classList.add('hidden');
      w.style.transform = ''; w.style.opacity = '';
      if (st.maximized && st.saved) { restoreGeo(w); st.maximized = false; w.classList.remove('maximized'); }
    });
    w.style.transform = 'scale(.94)';
    w.style.opacity = '0';
    syncDash();
    toast('Закрито: ' + title(w));
  }

  function saveGeo(w) {
    state[w.id].saved = { l: w.style.left, t: w.style.top, wd: w.style.width, h: w.style.height };
  }
  function restoreGeo(w) {
    var g = state[w.id].saved;
    if (!g) return;
    w.style.left = g.l; w.style.top = g.t; w.style.width = g.wd; w.style.height = g.h;
  }
  function setGeo(w, l, t, wd, h) {
    anim(w, 'anim-geo', 340);
    w.style.left = l + 'px'; w.style.top = t + 'px';
    w.style.width = wd + 'px'; w.style.height = h + 'px';
  }

  function toggleMax(w) {
    var st = state[w.id];
    if (st.maximized) {
      anim(w, 'anim-geo', 340);
      restoreGeo(w);
      st.maximized = false;
      w.classList.remove('maximized');
      toast('Відновлено: ' + title(w));
    } else {
      saveGeo(w);
      setGeo(w, 0, TOPBAR, screen.clientWidth, screen.clientHeight - TOPBAR);
      st.maximized = true;
      w.classList.add('maximized');
      toast('Розгорнуто: ' + title(w));
    }
    focusWindow(w);
  }

  function snap(w, side) {
    var st = state[w.id];
    if (!st.maximized) saveGeo(w);
    var halfW = Math.round(screen.clientWidth / 2);
    setGeo(w, side === 'left' ? 0 : halfW, TOPBAR, halfW, screen.clientHeight - TOPBAR);
    st.maximized = true;
    w.classList.add('maximized');
    focusWindow(w);
    toast((side === 'left' ? 'Ліва' : 'Права') + ' половина: ' + title(w));
  }

  /* --- Перетягування --- */
  wins.forEach(function (w) {
    var handle = w.querySelector('[data-drag]');
    var dragging = false, sx = 0, sy = 0, ox = 0, oy = 0;

    w.addEventListener('pointerdown', function () { if (!overview) focusWindow(w); });

    handle.addEventListener('pointerdown', function (e) {
      if (overview || e.target.closest('button')) return;
      dragging = true;
      sx = e.clientX; sy = e.clientY; ox = w.offsetLeft; oy = w.offsetTop;
      handle.setPointerCapture(e.pointerId);
      handle.style.cursor = 'grabbing';
      hideCtx();
      e.preventDefault();
    });
    handle.addEventListener('pointermove', function (e) {
      if (!dragging) return;
      if (state[w.id].maximized) {
        /* витягування розгорнутого вікна відновлює його розмір під курсором */
        var g = state[w.id].saved;
        state[w.id].maximized = false;
        w.classList.remove('maximized');
        w.style.width = g.wd; w.style.height = g.h;
        ox = e.clientX - w.offsetWidth / 2; oy = TOPBAR + 4;
        sx = e.clientX; sy = e.clientY;
      }
      var maxX = screen.clientWidth - w.offsetWidth;
      var maxY = screen.clientHeight - 44;
      w.style.left = Math.min(maxX, Math.max(0, ox + e.clientX - sx)) + 'px';
      w.style.top = Math.min(maxY, Math.max(TOPBAR, oy + e.clientY - sy)) + 'px';
    });
    function stop(e) {
      if (!dragging) return;
      dragging = false; handle.style.cursor = '';
      try { handle.releasePointerCapture(e.pointerId); } catch (err) { /* noop */ }
    }
    handle.addEventListener('pointerup', stop);
    handle.addEventListener('pointercancel', stop);

    handle.addEventListener('contextmenu', function (e) {
      e.preventDefault();
      if (!overview) showCtx(w, e);
    });

    var closeBtn = w.querySelector('.winbar .close');
    if (closeBtn) closeBtn.addEventListener('click', function (e) { e.stopPropagation(); closeWin(w); });

    var maxBtn = w.querySelector('.winbar .maximize');
    if (maxBtn) maxBtn.addEventListener('click', function (e) { e.stopPropagation(); if (!overview) toggleMax(w); });

    var ovClose = w.querySelector('.ov-close');
    if (ovClose) ovClose.addEventListener('click', function (e) {
      e.stopPropagation();
      closeWin(w);
      setTimeout(function () { if (overview) layoutOverview(); }, 240);
    });

    w.addEventListener('click', function () {
      if (!overview) return;
      setOverview(false);
      focusWindow(w);
    });
  });

  /* --- Контекстне меню --- */
  function showCtx(w, e) {
    ctxTarget = w;
    ctxMenu.querySelector('[data-ctx="maximize"]').textContent = state[w.id].maximized ? 'Відновити' : 'Розгорнути';
    var r = screen.getBoundingClientRect();
    var x = Math.min(e.clientX - r.left, screen.clientWidth - 228);
    var y = Math.min(e.clientY - r.top, screen.clientHeight - 210);
    ctxMenu.style.left = Math.max(4, x) + 'px';
    ctxMenu.style.top = Math.max(TOPBAR + 4, y) + 'px';
    ctxMenu.classList.add('open');
  }
  function hideCtx() { ctxMenu.classList.remove('open'); ctxTarget = null; }

  ctxMenu.addEventListener('click', function (e) {
    var b = e.target.closest('[data-ctx]');
    if (!b || !ctxTarget) return;
    var w = ctxTarget, act = b.dataset.ctx;
    hideCtx();
    if (act === 'minimize') minimize(w);
    else if (act === 'maximize') toggleMax(w);
    else if (act === 'left') snap(w, 'left');
    else if (act === 'right') snap(w, 'right');
    else if (act === 'close') closeWin(w);
  });
  screen.addEventListener('pointerdown', function (e) {
    if (!e.target.closest('#ctxMenu')) hideCtx();
  });

  /* --- Док --- */
  dash.querySelectorAll('[data-launch]').forEach(function (item) {
    item.addEventListener('click', function () {
      var w = document.getElementById(item.dataset.launch);
      var st = state[w.id];
      if (overview) setOverview(false);
      if (!st.open) { launch(w); return; }
      if (st.minimized) { st.minimized = false; appear(w, item, 'Відновлено: ' + title(w)); return; }
      if (w.classList.contains('active')) minimize(w);
      else focusWindow(w);
    });
  });
  dash.querySelectorAll('.dash-item:not([data-launch])').forEach(function (b) {
    b.addEventListener('click', function () { toast('Застосунок не встановлено'); });
  });

  /* --- Огляд --- */
  function visibleWins() {
    return wins.filter(function (w) { return state[w.id].open && !state[w.id].minimized; });
  }

  function layoutOverview() {
    var list = visibleWins();
    var n = list.length;
    if (!n) return;
    var cols = Math.ceil(Math.sqrt(n));
    var rows = Math.ceil(n / cols);
    var padX = 60, padTop = 118, padBottom = 88, gap = 30;
    var aw = screen.clientWidth - padX * 2;
    var ah = screen.clientHeight - padTop - padBottom;
    var cw = (aw - gap * (cols - 1)) / cols;
    var ch = (ah - gap * (rows - 1)) / rows;

    list.forEach(function (w, i) {
      var col = i % cols, row = Math.floor(i / cols);
      var ww = w.offsetWidth, wh = w.offsetHeight;
      var k = Math.min(cw / ww, ch / wh, .92);
      var tx = padX + col * (cw + gap) + (cw - ww * k) / 2 - w.offsetLeft;
      var ty = padTop + row * (ch + gap) + (ch - wh * k) / 2 - w.offsetTop;
      w.classList.add('anim-tf');
      w.style.transform = 'translate(' + tx.toFixed(1) + 'px,' + ty.toFixed(1) + 'px) scale(' + k.toFixed(3) + ')';
    });
  }

  function setOverview(on) {
    if (on === overview) return;
    overview = on;
    hideCtx();
    if (on) {
      screen.classList.add('overview');
      ovBtn.classList.add('active');
      wall.style.transform = 'scale(1.06)';
      wall.style.filter = 'saturate(.9)';
      layoutOverview();
    } else {
      screen.classList.remove('overview');
      ovBtn.classList.remove('active');
      wall.style.transform = '';
      wall.style.filter = '';
      visibleWins().forEach(function (w) {
        w.classList.add('anim-tf');
        w.style.transform = '';
        setTimeout(function () { w.classList.remove('anim-tf'); }, 460);
      });
    }
  }

  ovBtn.addEventListener('click', function () { setOverview(!overview); });
  window.addEventListener('resize', function () { if (overview) layoutOverview(); });

  screen.querySelectorAll('.ws-thumb').forEach(function (t) {
    t.addEventListener('click', function () {
      screen.querySelectorAll('.ws-thumb').forEach(function (x) { x.classList.remove('on'); });
      t.classList.add('on');
      toast('Робоча область перемкнена (демонстрація)');
    });
  });

  /* =======================================================================
     10. LAUNCHER — СІТКА ЗАСТОСУНКІВ
     ======================================================================= */
  var launcher = document.getElementById('launcher');
  var launcherTrack = document.getElementById('launcherTrack');
  var launcherDots = document.getElementById('launcherDots');
  var launcherCats = document.getElementById('launcherCats');
  var launcherBtn = document.getElementById('launcherBtn');
  var launcherOpen = false;
  var launcherPage = 0;
  var APPS_PER_PAGE = 24; // 6 cols × 4 rows

  var ICONS = {
    folder: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2.5 5h5l1.5 2h8.5v7.5a1 1 0 01-1 1h-13a1 1 0 01-1-1z"/></svg>',
    terminal: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2.5" y="3.5" width="15" height="13" rx="2.5"/><path d="M6 8l2.5 2L6 12M10.5 12.5h4"/></svg>',
    edit: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M12.5 3.5l4 4M4 12l8.5-8.5 4 4L8 16H4v-4z"/></svg>',
    calc: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="4" y="2" width="12" height="16" rx="2.5"/><path d="M7 6h6M7 10h2M11 10h2M7 13h2M11 13h2"/></svg>',
    archive: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2.5" y="3" width="15" height="4" rx="1.5"/><path d="M4 7v8.5a1.5 1.5 0 001.5 1.5h9a1.5 1.5 0 001.5-1.5V7M8 11h4"/></svg>',
    screenshot: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="4.5" width="16" height="11" rx="2.5"/><circle cx="10" cy="10" r="3"/><circle cx="14.5" cy="6.5" r=".8" fill="currentColor" stroke="none"/></svg>',
    doc: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5 2.5h7l4 4V17a.5.5 0 01-.5.5h-10a.5.5 0 01-.5-.5V3a.5.5 0 01.5-.5z"/><path d="M12 2.5v4h4M7.5 10h5M7.5 13h5"/></svg>',
    sheet: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="3" width="14" height="14" rx="2.5"/><path d="M3 7h14M3 11h14M3 15h14M7 3v14M12 3v14"/></svg>',
    slides: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2.5" y="3.5" width="15" height="11" rx="2"/><path d="M6 17h8M10 14.5V17"/><rect x="5.5" y="6" width="9" height="6" rx="1"/></svg>',
    pdf: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5 2.5h7l4 4V17a.5.5 0 01-.5.5h-10a.5.5 0 01-.5-.5V3a.5.5 0 01.5-.5z"/><path d="M12 2.5v4h4"/><path d="M7 12.5h1.5a1.2 1.2 0 000-2.4H7v4" stroke-width="1.3"/></svg>',
    browser: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="10" cy="10" r="7.5"/><path d="M2.5 10h15M10 2.5c-2.5 2.5-2.5 12.5 0 15M10 2.5c2.5 2.5 2.5 12.5 0 15"/></svg>',
    mail: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2.5" y="4.5" width="15" height="11" rx="2.5"/><path d="M2.5 6.5L10 11l7.5-4.5"/></svg>',
    chat: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 4.5h14a.5.5 0 01.5.5v8a.5.5 0 01-.5.5H8l-3.5 3v-3H3a.5.5 0 01-.5-.5V5a.5.5 0 01.5-.5z"/><path d="M6.5 8h7M6.5 10.5h4"/></svg>',
    download: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M10 3v10M6.5 9.5L10 13l3.5-3.5M3.5 15h13"/></svg>',
    music: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M7.5 14V5l7-1.5V13"/><circle cx="5.8" cy="14.2" r="1.8"/><circle cx="12.8" cy="12.7" r="1.8"/></svg>',
    photos: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="3.5" width="16" height="13" rx="2.5"/><circle cx="7" cy="8" r="1.5"/><path d="M2.5 14l4.5-4 3.5 3 2.5-2 4.5 3"/></svg>',
    video: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="4" width="11.5" height="12" rx="2.5"/><path d="M13.5 8l4-2.5v9L13.5 12"/></svg>',
    camera: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2.5 6.5h2.5l1.5-2h7l1.5 2h2.5v9.5h-15z"/><circle cx="10" cy="10.5" r="3"/></svg>',
    settings: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="10" cy="10" r="2.8"/><path d="M10 2v2.2M10 15.8V18M18 10h-2.2M4.2 10H2M15.7 4.3l-1.6 1.6M5.9 14.1l-1.6 1.6M15.7 15.7l-1.6-1.6M5.9 5.9L4.3 4.3"/></svg>',
    sysmon: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2.5" y="3" width="15" height="11" rx="2.5"/><path d="M5 16.5h10M6 10.5l2.5-3.5 2 2.5 2-3 2 4"/></svg>',
    disks: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="10" cy="10" r="7.5"/><circle cx="10" cy="10" r="2"/><circle cx="10" cy="10" r="5" stroke-dasharray="2 3"/></svg>',
    log: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 3h12a.5.5 0 01.5.5v13a.5.5 0 01-.5.5H4a.5.5 0 01-.5-.5V3.5A.5.5 0 014 3z"/><path d="M6.5 7h7M6.5 10h5M6.5 13h3"/></svg>',
    update: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3.5 10a6.5 6.5 0 0112.7-2M16.5 10a6.5 6.5 0 01-12.7 2"/><path d="M16.5 4v4h-4M3.5 16v-4h4"/></svg>',
    clockapp: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="10" cy="10" r="7.5"/><path d="M10 5.5V10l3 2.5"/></svg>',
    fonts: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 15l4-10h4l4 10M5.5 11.5h9"/></svg>',
    maps: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M10 17.5s-6-5.5-6-9.5a6 6 0 0112 0c0 4-6 9.5-6 9.5z"/><circle cx="10" cy="8" r="2"/></svg>',
    weather: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="3"/><path d="M8 2.5v1.5M8 12v1.5M2.5 8H4M12 8h1.5M4.1 4.1l1 1M11.9 11.9l1 1M4.1 11.9l1-1M11.9 4.1l1-1"/><path d="M11 13a4 4 0 11-4.8-.7"/></svg>',
    contacts: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="10" cy="7.5" r="3"/><path d="M4 16.5c0-3.3 2.7-6 6-6s6 2.7 6 6"/></svg>',
    recorder: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="7.5" y="2.5" width="5" height="9" rx="2.5"/><path d="M5 10a5 5 0 0010 0M10 15v2.5"/></svg>',
    wallpaper: '<svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2.5" y="2.5" width="15" height="15" rx="2.5"/><path d="M2.5 13l4.5-4 3 2.5 2.5-2 5 4"/><circle cx="13.5" cy="6.5" r="1.5"/></svg>'
  };

  var APPS = [
    { name: 'Файли', cat: 'utilities', bg: 'linear-gradient(150deg,var(--ac-3),var(--ac-5))', icon: 'folder' },
    { name: 'Термінал', cat: 'utilities', bg: 'linear-gradient(150deg,#4a4640,#23201c)', icon: 'terminal' },
    { name: 'Текстовий редактор', cat: 'utilities', bg: 'linear-gradient(150deg,#5aa3a0,#2c6e6a)', icon: 'edit' },
    { name: 'Калькулятор', cat: 'utilities', bg: 'linear-gradient(150deg,#e0924a,#a85520)', icon: 'calc' },
    { name: 'Архіватор', cat: 'utilities', bg: 'linear-gradient(150deg,#8a7a60,#5c4e38)', icon: 'archive' },
    { name: 'Знімок екрана', cat: 'utilities', bg: 'linear-gradient(150deg,#9b7acb,#6840a0)', icon: 'screenshot' },

    { name: 'Документи', cat: 'office', bg: 'linear-gradient(150deg,#5a8fd4,#2d5ba0)', icon: 'doc' },
    { name: 'Таблиця', cat: 'office', bg: 'linear-gradient(150deg,#4ca070,#287050)', icon: 'sheet' },
    { name: 'Презентація', cat: 'office', bg: 'linear-gradient(150deg,#d4845a,#a05828)', icon: 'slides' },
    { name: 'PDF Reader', cat: 'office', bg: 'linear-gradient(150deg,#d45a5a,#a02828)', icon: 'pdf' },

    { name: 'Браузер', cat: 'internet', bg: 'linear-gradient(150deg,#5a7ad4,#3050a0)', icon: 'browser' },
    { name: 'Пошта', cat: 'internet', bg: 'linear-gradient(150deg,#7fae94,#2f7a56)', icon: 'mail' },
    { name: 'Чат', cat: 'internet', bg: 'linear-gradient(150deg,#b07ad4,#7840a0)', icon: 'chat' },
    { name: 'Завантаження', cat: 'internet', bg: 'linear-gradient(150deg,#5aafcf,#28789a)', icon: 'download' },

    { name: 'Музика', cat: 'media', bg: 'linear-gradient(150deg,#c9a06a,#8a5c2a)', icon: 'music' },
    { name: 'Фотографії', cat: 'media', bg: 'linear-gradient(150deg,#d47aa0,#a04068)', icon: 'photos' },
    { name: 'Відео', cat: 'media', bg: 'linear-gradient(150deg,#d45a6a,#a02838)', icon: 'video' },
    { name: 'Камера', cat: 'media', bg: 'linear-gradient(150deg,#8a6ac9,#5a3a8a)', icon: 'camera' },

    { name: 'Параметри', cat: 'system', bg: 'linear-gradient(150deg,#8a8377,#4c4740)', icon: 'settings' },
    { name: 'Монітор системи', cat: 'system', bg: 'linear-gradient(150deg,#5aaf7a,#287840)', icon: 'sysmon' },
    { name: 'Диски', cat: 'system', bg: 'linear-gradient(150deg,#7a8a9a,#4a5a6a)', icon: 'disks' },
    { name: 'Журнал', cat: 'system', bg: 'linear-gradient(150deg,#9a8a6a,#6a5a3a)', icon: 'log' },
    { name: 'Оновлення', cat: 'system', bg: 'linear-gradient(150deg,#5a8ad4,#2860a0)', icon: 'update' },
    { name: 'Годинник', cat: 'system', bg: 'linear-gradient(150deg,#5aafa0,#287868)', icon: 'clockapp' },

    { name: 'Шрифти', cat: 'utilities', bg: 'linear-gradient(150deg,#7a7a8a,#4a4a5a)', icon: 'fonts' },
    { name: 'Карти', cat: 'internet', bg: 'linear-gradient(150deg,#6aaf6a,#3a8a3a)', icon: 'maps' },
    { name: 'Погода', cat: 'utilities', bg: 'linear-gradient(150deg,#6aafd4,#3a80a0)', icon: 'weather' },
    { name: 'Контакти', cat: 'internet', bg: 'linear-gradient(150deg,#af8a5a,#806030)', icon: 'contacts' },
    { name: 'Записи', cat: 'media', bg: 'linear-gradient(150deg,#af5a7a,#803048)', icon: 'recorder' },
    { name: 'Шпалери', cat: 'system', bg: 'linear-gradient(150deg,#8aaf5a,#608030)', icon: 'wallpaper' }
  ];

  var currentCat = 'all';

  function filteredApps() {
    if (currentCat === 'all') return APPS.slice();
    return APPS.filter(function (a) { return a.cat === currentCat; });
  }

  function buildLauncher() {
    var apps = filteredApps();
    var pages = Math.max(1, Math.ceil(apps.length / APPS_PER_PAGE));
    launcherTrack.innerHTML = '';
    for (var p = 0; p < pages; p++) {
      var page = document.createElement('div');
      page.className = 'launcher-page';
      var start = p * APPS_PER_PAGE;
      var end = Math.min(start + APPS_PER_PAGE, apps.length);
      for (var i = start; i < end; i++) {
        var app = apps[i];
        var btn = document.createElement('button');
        btn.className = 'launcher-app';
        btn.innerHTML =
          '<span class="app-icon" style="background:' + app.bg + '">' +
          (ICONS[app.icon] || '') +
          '</span>' + app.name;
        (function (a) {
          btn.addEventListener('click', function () {
            setLauncher(false);
            toast('Запущено: ' + a.name);
          });
        })(app);
        page.appendChild(btn);
      }
      launcherTrack.appendChild(page);
    }
    /* dots */
    launcherDots.innerHTML = '';
    for (var d = 0; d < pages; d++) {
      var dot = document.createElement('button');
      dot.setAttribute('aria-current', d === 0 ? 'true' : 'false');
      (function (idx) {
        dot.addEventListener('click', function () { goToPage(idx); });
      })(d);
      launcherDots.appendChild(dot);
    }
    launcherPage = 0;
    launcherTrack.style.transform = 'translateX(0)';
  }

  function goToPage(idx) {
    var apps = filteredApps();
    var pages = Math.max(1, Math.ceil(apps.length / APPS_PER_PAGE));
    if (idx < 0 || idx >= pages) return;
    launcherPage = idx;
    launcherTrack.style.transform = 'translateX(-' + (idx * 100) + '%)';
    Array.prototype.forEach.call(launcherDots.children, function (d, i) {
      d.setAttribute('aria-current', String(i === idx));
    });
  }

  function setLauncher(on) {
    if (on === launcherOpen) return;
    launcherOpen = on;
    if (on) {
      if (overview) setOverview(false);
      buildLauncher();
      launcher.classList.add('open');
      wall.style.transform = 'scale(1.04)';
      wall.style.filter = 'saturate(.85) brightness(.9)';
    } else {
      launcher.classList.remove('open');
      if (!overview) {
        wall.style.transform = '';
        wall.style.filter = '';
      }
    }
  }

  /* категорії */
  if (launcherCats) {
    launcherCats.addEventListener('click', function (e) {
      var btn = e.target.closest('button');
      if (!btn || !btn.dataset.cat) return;
      currentCat = btn.dataset.cat;
      launcherCats.querySelectorAll('button').forEach(function (b) {
        b.setAttribute('aria-selected', String(b === btn));
      });
      buildLauncher();
    });
  }

  /* кнопка в доку */
  if (launcherBtn) {
    launcherBtn.addEventListener('click', function () {
      setLauncher(!launcherOpen);
    });
  }

  /* Escape — закрити що відкрито */
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') {
      if (launcherOpen) setLauncher(false);
      else if (overview) setOverview(false);
      else hideCtx();
    }
  });

  /* клік по пустому простору лаунчера закриває його */
  if (launcher) {
    launcher.addEventListener('click', function (e) {
      if (e.target === launcher || e.target.classList.contains('launcher-viewport') || e.target.classList.contains('launcher-track')) {
        setLauncher(false);
      }
    });
  }

  syncDash();
  focusWindow(document.getElementById('w1'));
})();
