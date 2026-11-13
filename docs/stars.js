// GitHub stars badge: fetches the star count, counts up to it, fills the star
// in step with the count, then bursts a few star particles. Vanilla port of
// the animate-ui GithubStars primitive.
(() => {
  'use strict';

  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const compact = new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 });
  const format = n => (n < 1000 ? String(n) : compact.format(n));
  const requests = new Map();

  function stars(slug) {
    if (requests.has(slug)) return requests.get(slug);
    const key = 'gh-stars:' + slug;
    let cached = null;
    try { cached = JSON.parse(sessionStorage.getItem(key)); } catch {}
    const p = cached && Date.now() - cached.at < 10 * 60 * 1000
      ? Promise.resolve(cached.count)
      : fetch('https://api.github.com/repos/' + slug, { headers: { Accept: 'application/vnd.github+json' } })
          .then(r => (r.ok ? r.json() : Promise.reject(r.status)))
          .then(d => {
            const count = d.stargazers_count | 0;
            try { sessionStorage.setItem(key, JSON.stringify({ count, at: Date.now() })); } catch {}
            return count;
          });
    requests.set(slug, p);
    return p;
  }

  function burst(host) {
    const n = 6;
    for (let i = 0; i < n; i++) {
      const p = host.firstElementChild.cloneNode(true);
      p.classList.add('gh-stars-particle');
      const a = (i / n) * Math.PI * 2 - Math.PI / 2 + (Math.random() - 0.5) * 0.6;
      const d = 14 + Math.random() * 8;
      p.style.setProperty('--dx', (Math.cos(a) * d).toFixed(1) + 'px');
      p.style.setProperty('--dy', (Math.sin(a) * d).toFixed(1) + 'px');
      p.style.setProperty('--r', ((Math.random() - 0.5) * 120).toFixed(0) + 'deg');
      p.addEventListener('animationend', () => p.remove());
      host.appendChild(p);
    }
  }

  function animate(el, target) {
    const num = el.querySelector('.gh-stars-number');
    const host = el.querySelector('.gh-stars-particles');
    const done = () => {
      num.textContent = format(target);
      el.style.setProperty('--fill', '1');
      el.classList.add('is-complete');
      const link = el.closest('a');
      if (link && !link.querySelector('span:not([class])')) link.setAttribute('aria-label', `dither-kit-rs on GitHub, ${target} stars`);
    };
    el.classList.add('is-loaded');
    if (reduced || target === 0) return done();
    const ms = Math.min(1800, 600 + Math.log10(target + 1) * 400);
    const start = performance.now();
    const step = now => {
      const t = Math.min(1, (now - start) / ms);
      const e = 1 - Math.pow(1 - t, 4);
      num.textContent = format(Math.round(target * e));
      el.style.setProperty('--fill', e.toFixed(3));
      if (t < 1) return requestAnimationFrame(step);
      done();
      burst(host);
    };
    requestAnimationFrame(step);
  }

  document.querySelectorAll('[data-gh-stars]').forEach(el => {
    stars(el.dataset.ghStars)
      .then(count => animate(el, count))
      .catch(() => el.classList.add('is-failed'));
  });
})();
