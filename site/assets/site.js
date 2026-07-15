// loadr.io — tiny static-site runtime: syntax tinting, tabs, copy buttons, nav.

(function () {
  "use strict";

  // -------------------------------------------------------------------------
  // Minimal syntax highlighter (yaml / js / bash / console)
  // -------------------------------------------------------------------------
  function esc(s) {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  var rules = {
    yaml: [
      [/(^|\n)(\s*#[^\n]*)/g, function (m, a, b) { return a + span("tok-c", b); }],
      [/(^|\n)(\s*-?\s*)([A-Za-z0-9_.\-{}$()"']+)(:)(\s|\n|$)/g, function (m, a, ind, key, colon, tail) {
        return a + ind + span("tok-k", key) + span("tok-p", colon) + tail;
      }],
      [/("(?:[^"\\]|\\.)*"|'[^']*')/g, function (m, s) { return span("tok-s", s); }],
      [/\$\{[^}]*\}/g, function (m) { return span("tok-f", m); }],
      [/\b(\d+(?:\.\d+)?(?:ms|s|m|h)?)\b/g, function (m) { return span("tok-n", m); }],
    ],
    js: [
      [/(\/\/[^\n]*)/g, function (m) { return span("tok-c", m); }],
      [/('(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*"|`(?:[^`\\]|\\.)*`)/g, function (m) { return span("tok-s", m); }],
      [/\b(import|from|export|function|const|let|var|return|if|else|new|default|async|await)\b/g,
        function (m) { return span("tok-k", m); }],
      [/\b(\d+(?:\.\d+)?)\b/g, function (m) { return span("tok-n", m); }],
      [/\b(http|check|sleep|group|session|crypto|console|__ENV)\b/g, function (m) { return span("tok-f", m); }],
    ],
    bash: [
      [/(^|\n)(\s*#[^\n]*)/g, function (m, a, b) { return a + span("tok-c", b); }],
      [/(^|\n)(\$)(\s)/g, function (m, a, d, sp) { return a + span("tok-red", d) + sp; }],
      // Flags must start at a word boundary (space / line-start / paren) so the
      // rule can't match the "-red" inside an already-inserted tok-red sentinel.
      [/(^|[\s(])(--?[a-z][\w-]*)/g, function (m, pre, flag) { return pre + span("tok-f", flag); }],
      [/\b(loadr|cargo|docker|helm|kubectl|curl|tar)\b/g, function (m) { return span("tok-k", m); }],
    ],
    console: [
      [/(✓[^\n]*)/g, function (m) { return span("tok-ok", m); }],
      [/(✗[^\n]*)/g, function (m) { return span("tok-red", m); }],
      [/(^|\n)(\s*[a-z_0-9.]+\.{2,}:)/g, function (m, a, b) { return a + span("tok-k", b); }],
      [/\b(avg|min|med|max|p\(\d+(?:\.\d+)?\))=/g, function (m) { return span("tok-dim", m); }],
      [/(^|\n)(\$\s[^\n]*)/g, function (m, a, b) { return a + span("tok-f", b); }],
    ],
  };

  // Sentinel chars survive HTML escaping, then become spans.
  var S1 = "\u0001", S2 = "\u0002", S3 = "\u0003";

  function span(cls, text) {
    return S1 + cls + S2 + text + S3;
  }

  function finalize(s) {
    return esc(s)
      .replace(/\u0001([\w-]+)\u0002/g, '<span class="$1">')
      .replace(/\u0003/g, "</span>");
  }

  document.querySelectorAll("pre code[data-lang]").forEach(function (code) {
    var lang = code.getAttribute("data-lang");
    var set = rules[lang];
    if (!set) return;
    var text = code.textContent;
    set.forEach(function (rule) {
      text = text.replace(rule[0], rule[1]);
    });
    code.innerHTML = finalize(text);
  });

  // -------------------------------------------------------------------------
  // Copy buttons
  // -------------------------------------------------------------------------
  document.querySelectorAll("[data-copy]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var target = document.querySelector(btn.getAttribute("data-copy"));
      if (!target) return;
      navigator.clipboard.writeText(target.textContent.trim()).then(function () {
        var old = btn.textContent;
        btn.textContent = "copied!";
        setTimeout(function () { btn.textContent = old; }, 1400);
      });
    });
  });

  // -------------------------------------------------------------------------
  // Tabs
  // -------------------------------------------------------------------------
  document.querySelectorAll("[data-tabs]").forEach(function (root) {
    var buttons = root.querySelectorAll(".tabbtn");
    var panels = root.querySelectorAll(".tabpanel");
    buttons.forEach(function (btn, i) {
      btn.addEventListener("click", function () {
        buttons.forEach(function (b) { b.setAttribute("aria-selected", "false"); });
        panels.forEach(function (p) { p.classList.add("hidden"); });
        btn.setAttribute("aria-selected", "true");
        panels[i].classList.remove("hidden");
      });
    });
  });

  // -------------------------------------------------------------------------
  // Mobile nav
  // -------------------------------------------------------------------------
  var navBtn = document.getElementById("navToggle");
  var navMenu = document.getElementById("navMenu");
  if (navBtn && navMenu) {
    navBtn.addEventListener("click", function () {
      navMenu.classList.toggle("hidden");
    });
    navMenu.querySelectorAll("a").forEach(function (a) {
      a.addEventListener("click", function () { navMenu.classList.add("hidden"); });
    });
  }

  // -------------------------------------------------------------------------
  // Active nav highlight — the nav is one shared partial, so the active item
  // is marked at runtime from the current path (data-nav on each top link).
  // -------------------------------------------------------------------------
  (function () {
    var p = location.pathname;
    var key = p.indexOf("/demos") === 0 ? "demos"
            : p.indexOf("/plugins") === 0 ? "plugins"
            : p.indexOf("/docs") === 0 ? "docs"
            : p.indexOf("/download") === 0 ? "download"
            : "";
    if (!key) return;
    document.querySelectorAll('[data-nav="' + key + '"]').forEach(function (a) {
      a.classList.add("text-flare", "font-semibold");
      a.classList.remove("text-smoke");
    });
  })();

  // -------------------------------------------------------------------------
  // Animated fake live chart in the web UI mockup
  // -------------------------------------------------------------------------
  var chart = document.getElementById("uiChart");
  var reduceMotion = window.matchMedia
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (chart && !reduceMotion) {
    var bars = chart.querySelectorAll(".chartbar");
    setInterval(function () {
      bars.forEach(function (b) {
        var h = 22 + Math.random() * 74;
        b.style.height = h + "%";
      });
    }, 900);
  }

  // -------------------------------------------------------------------------
  // Desktop screenshot slideshow (crossfade + dots + arrows, auto-advancing,
  // paused on hover/focus; honours prefers-reduced-motion).
  // -------------------------------------------------------------------------
  document.querySelectorAll("[data-desk-show]").forEach(function (root) {
    var slides = root.querySelectorAll("[data-desk-slide]");
    if (slides.length < 2) return;
    var caption = root.querySelector("[data-desk-caption]");
    var dotsWrap = root.querySelector("[data-desk-dots]");
    var i = 0, timer = null, dots = [];

    slides.forEach(function (_, idx) {
      var d = document.createElement("button");
      d.type = "button";
      d.setAttribute("aria-label", "Show screenshot " + (idx + 1));
      d.addEventListener("click", function () { go(idx); restart(); });
      dotsWrap.appendChild(d);
      dots.push(d);
    });

    function go(n) {
      i = (n + slides.length) % slides.length;
      slides.forEach(function (s, idx) {
        s.style.opacity = idx === i ? "1" : "0";
        s.style.zIndex = idx === i ? "1" : "0";
      });
      dots.forEach(function (d, idx) {
        d.className = "h-2 rounded-full transition-all " + (idx === i ? "w-5 bg-flare" : "w-2 bg-edge-bright hover:bg-smoke");
      });
      if (caption) caption.textContent = slides[i].getAttribute("data-caption") || "";
    }
    function start() { if (!reduceMotion && !timer) timer = setInterval(function () { go(i + 1); }, 4200); }
    function stop() { if (timer) { clearInterval(timer); timer = null; } }
    function restart() { stop(); start(); }

    root.querySelector("[data-desk-prev]").addEventListener("click", function () { go(i - 1); restart(); });
    root.querySelector("[data-desk-next]").addEventListener("click", function () { go(i + 1); restart(); });
    root.addEventListener("mouseenter", stop);
    root.addEventListener("mouseleave", start);
    root.addEventListener("focusin", stop);
    root.addEventListener("focusout", start);

    go(0);
    start();
  });

  // -------------------------------------------------------------------------
  // Canvas build-up walkthroughs — manual step-through (prev/next + dots),
  // no auto-advance. Each frame carries a data-cap caption.
  // -------------------------------------------------------------------------
  document.querySelectorAll("[data-walk]").forEach(function (root) {
    var frames = [].slice.call(root.querySelectorAll("[data-fr]"));
    if (frames.length < 2) return;
    var cap = root.querySelector("[data-cap]");
    var dotsWrap = root.querySelector("[data-dots]");
    var i = 0;
    var dots = frames.map(function (_, idx) {
      var d = document.createElement("button");
      d.type = "button";
      d.setAttribute("aria-label", "Step " + (idx + 1));
      d.className = "h-1.5 w-1.5 rounded-full bg-edge-bright";
      d.addEventListener("click", function () { go(idx); });
      dotsWrap.appendChild(d);
      return d;
    });
    function go(n) {
      i = (n + frames.length) % frames.length;
      frames.forEach(function (f, idx) { f.style.opacity = idx === i ? "1" : "0"; });
      dots.forEach(function (d, idx) {
        d.className = "h-1.5 rounded-full transition-all " + (idx === i ? "w-4 bg-flare" : "w-1.5 bg-edge-bright");
      });
      if (cap) cap.textContent = frames[i].getAttribute("data-cap") || "";
    }
    root.querySelector("[data-pv]").addEventListener("click", function () { go(i - 1); });
    root.querySelector("[data-nx]").addEventListener("click", function () { go(i + 1); });
    // Rest on the fully-composed pattern so each card is distinct at a glance;
    // frame 0 is "scenario + first request" for every pattern and reads as a
    // duplicate. Prev/next still walks the build-up from the start.
    go(frames.length - 1);
  });
})();
