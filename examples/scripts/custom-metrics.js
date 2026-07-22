// Custom business metrics for examples/39-custom-metrics.yaml.
//
// One exported iteration (`exec: shop`) that drives a small purchase funnel and
// records four user-defined metrics alongside loadr's built-in HTTP stats.
import http from 'loadr/http';
import { check, sleep } from 'loadr';

export function shop() {
  // Browse — record whether the catalogue response came from cache.
  const list = http.get('/products?limit=20', { tags: { endpoint: 'products' } });
  check(list, { 'list ok': (r) => r.status === 200 });
  session.rateAdd('cache_hit', list.headers['X-Cache'] === 'HIT');

  // Add to cart.
  const sku = `W-${Math.ceil(Math.random() * 50)}`;
  http.post('/cart', JSON.stringify({ sku, qty: 1 }), {
    headers: { 'Content-Type': 'application/json' },
    tags: { endpoint: 'cart' },
  });
  session.gaugeSet('inflight_orders', Number(session.vars.inflight || 0) + 1);

  // Checkout — time it ourselves and bank the revenue on success.
  const t0 = Date.now();
  const co = http.post('/checkout', null, { tags: { endpoint: 'checkout' } });
  session.trendAdd('checkout_latency', Date.now() - t0);
  if (co.status === 200) {
    const total = co.json() ? Number(co.json().total_usd || 0) : 0;
    session.counterAdd('revenue_usd', total);
  }
  session.gaugeSet('inflight_orders', Math.max(0, Number(session.vars.inflight || 1) - 1));

  sleep(Math.random() + 0.25);
}
