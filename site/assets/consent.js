/* loadr — Google Analytics (gtag.js) with Consent Mode v2 + a themed cookie
 * banner. Analytics stays in "denied" (cookieless) mode until the visitor
 * accepts. The choice is remembered in localStorage. Self-contained: injects
 * its own styles so it also works on the docs pages.
 *
 * Note: this governs the marketing/docs *website* only. The loadr tool itself
 * ships zero telemetry. */
(function () {
  'use strict';

  var GA_ID = 'G-BJWFJCC9X0';
  var KEY = 'loadr.consent'; // stored value: 'granted' | 'denied'

  // --- gtag bootstrap ------------------------------------------------------
  window.dataLayer = window.dataLayer || [];
  function gtag() { dataLayer.push(arguments); }
  window.gtag = gtag;
  gtag('js', new Date());

  // EU-safe defaults: deny storage until the visitor opts in.
  gtag('consent', 'default', {
    ad_storage: 'denied',
    ad_user_data: 'denied',
    ad_personalization: 'denied',
    analytics_storage: 'denied'
  });
  gtag('config', GA_ID, { anonymize_ip: true });

  // Load the GA library. With consent denied it sends only cookieless pings.
  var ga = document.createElement('script');
  ga.async = true;
  ga.src = 'https://www.googletagmanager.com/gtag/js?id=' + GA_ID;
  document.head.appendChild(ga);

  // --- consent persistence -------------------------------------------------
  function read() { try { return localStorage.getItem(KEY); } catch (e) { return null; } }
  function write(v) { try { localStorage.setItem(KEY, v); } catch (e) { /* ignore */ } }

  function grant() {
    gtag('consent', 'update', {
      ad_storage: 'granted',
      ad_user_data: 'granted',
      ad_personalization: 'granted',
      analytics_storage: 'granted'
    });
  }

  var prior = read();
  if (prior === 'granted') grant();
  if (prior === 'granted' || prior === 'denied') return; // already decided — no banner

  // --- themed banner -------------------------------------------------------
  function injectStyles() {
    if (document.getElementById('loadr-cookie-style')) return;
    var css =
      '.loadr-cookie{position:fixed;left:0;right:0;bottom:0;z-index:2147483000;' +
      'background:rgba(10,10,14,.97);border-top:1px solid #232330;' +
      '-webkit-backdrop-filter:blur(8px);backdrop-filter:blur(8px);' +
      'box-shadow:0 -8px 30px rgba(0,0,0,.5);animation:loadrCookieUp .25s ease}' +
      '@keyframes loadrCookieUp{from{transform:translateY(100%)}to{transform:translateY(0)}}' +
      '.loadr-cookie-inner{max-width:72rem;margin:0 auto;padding:14px 20px;display:flex;' +
      'gap:16px;align-items:center;justify-content:space-between;flex-wrap:wrap;' +
      'font-family:ui-sans-serif,system-ui,-apple-system,Segoe UI,Roboto,sans-serif}' +
      '.loadr-cookie-text{margin:0;color:#9ca3af;font-size:13px;line-height:1.5;max-width:46rem}' +
      '.loadr-cookie-text strong{color:#d6dae3;font-weight:600}' +
      '.loadr-cookie-link{color:#f87171;text-decoration:none;font-weight:600}' +
      '.loadr-cookie-link:hover{text-decoration:underline}' +
      '.loadr-cookie-actions{display:flex;gap:10px;flex-shrink:0}' +
      '.loadr-cookie-btn{font:inherit;font-size:13px;font-weight:600;padding:8px 18px;' +
      'border-radius:8px;cursor:pointer;border:1px solid #232330;transition:background .15s ease,border-color .15s ease,color .15s ease}' +
      '.loadr-cookie-decline{background:transparent;color:#9ca3af}' +
      '.loadr-cookie-decline:hover{color:#fff;border-color:#3a3a48}' +
      '.loadr-cookie-accept{background:#dc2626;color:#fff;border-color:#dc2626}' +
      '.loadr-cookie-accept:hover{background:#ef4444;border-color:#ef4444}' +
      '@media(max-width:560px){.loadr-cookie-inner{flex-direction:column;align-items:stretch}' +
      '.loadr-cookie-actions{width:100%}.loadr-cookie-btn{flex:1}}';
    var st = document.createElement('style');
    st.id = 'loadr-cookie-style';
    st.textContent = css;
    document.head.appendChild(st);
  }

  function buildBanner() {
    injectStyles();
    var bar = document.createElement('div');
    bar.className = 'loadr-cookie';
    bar.setAttribute('role', 'dialog');
    bar.setAttribute('aria-live', 'polite');
    bar.setAttribute('aria-label', 'Cookie consent');
    bar.innerHTML =
      '<div class="loadr-cookie-inner">' +
      '<p class="loadr-cookie-text">loadr.io uses a couple of cookies for anonymous ' +
      '<strong>Google Analytics</strong> — only to see which pages help. Nothing else, ' +
      'and the loadr tool itself ships <strong>zero telemetry</strong>. ' +
      '<a class="loadr-cookie-link" href="/cookies/">Learn more</a></p>' +
      '<div class="loadr-cookie-actions">' +
      '<button type="button" class="loadr-cookie-btn loadr-cookie-decline">Decline</button>' +
      '<button type="button" class="loadr-cookie-btn loadr-cookie-accept">Accept</button>' +
      '</div></div>';
    document.body.appendChild(bar);

    function close() { if (bar.parentNode) bar.parentNode.removeChild(bar); }
    bar.querySelector('.loadr-cookie-accept').addEventListener('click', function () {
      write('granted'); grant(); close();
    });
    bar.querySelector('.loadr-cookie-decline').addEventListener('click', function () {
      write('denied'); close();
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', buildBanner);
  } else {
    buildBanner();
  }
})();
