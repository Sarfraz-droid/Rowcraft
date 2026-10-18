'use strict';
document.querySelector('#copy-build').addEventListener('click', async event => { const button = event.currentTarget; try { await navigator.clipboard.writeText(document.querySelector('#copy-build').previousElementSibling.textContent); button.textContent = 'Copied!'; setTimeout(() => button.textContent = 'Copy commands', 2000); } catch { button.textContent = 'Select the commands to copy'; } });
const config = window.ROWCRAFT_CONFIG || {};
function verifiedUrl(value) { try {const url = new URL(value); return url.protocol === 'https:' ? url.href : null;} catch {return null;} }
const repo = verifiedUrl(config.repositoryUrl);
if (repo) { const github = document.querySelector('#github-link'); github.href = repo; github.target = '_blank'; github.rel = 'noopener noreferrer'; const source = document.querySelector('#source-link');source.href=repo;source.target='_blank';source.rel='noopener noreferrer'; const note = document.querySelector('#source-note'); const readme = document.createElement('a');readme.href=`${repo.replace(/\/$/,'')}`;readme.textContent='Read setup instructions in the source repository →';note.replaceChildren(readme); }
document.querySelectorAll('a[href="#source-guide"]').forEach(link => link.addEventListener('click', () => document.querySelector('#source-guide').open = true));
if (location.hash === '#source-guide') document.querySelector('#source-guide').open = true;
document.querySelector('#year').textContent = new Date().getFullYear();

// Default content remains visible if scripting or animation support is unavailable.
function setupPageTransitions() {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  if (!Element.prototype.animate) return;

  const active = new Map();
  const ease = 'cubic-bezier(0.16, 1, 0.3, 1)';
  let observer;

  function play(element, keyframes, options = {}) {
    if (!element || motion.matches || document.hidden) return;
    active.get(element)?.cancel();
    const animation = element.animate(keyframes, { duration: 560, easing: ease, ...options });
    active.set(element, animation);
    const release = () => { if (active.get(element) === animation) active.delete(element); };
    animation.finished.then(release, release);
  }

  function finishMotion() {
    for (const animation of active.values()) animation.cancel();
    active.clear();
  }

  // Assemble the opening workspace in reading order. Deep links and restored
  // scroll positions should land on their content without replaying the hero.
  if (!motion.matches && !location.hash && window.scrollY < 32) {
    const sequence = [
      ['.header', 0, -8],
      ['.hero-heading h1', 60, 16],
      ['.rust-credit', 170, 10],
      ['.hero-aside', 230, 12],
      ['.hero-actions', 300, 10],
      ['.platform-note', 300, 10],
      ['.preview-reveal', 380, 18],
    ];
    for (const [selector, delay, distance] of sequence) {
      play(document.querySelector(selector), [
        { opacity: 0.3, transform: `translateY(${distance}px)`, clipPath: 'inset(0 0 8% 0)' },
        { opacity: 1, transform: 'translateY(0)', clipPath: 'inset(0)' },
      ], { delay, duration: 620, fill: 'backwards' });
    }
  }

  if (!motion.matches && 'IntersectionObserver' in window) {
    observer = new IntersectionObserver(entries => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        observer.unobserve(entry.target);
        play(entry.target, [
          { opacity: 0.5, transform: 'translateY(18px)' },
          { opacity: 1, transform: 'translateY(0)' },
        ]);
        if (entry.target.matches('.benchmark')) {
          entry.target.querySelectorAll('.chart-rowcraft, .chart-datagrip').forEach((bar, index) => {
            play(bar, [
              { transform: 'scaleX(0)', opacity: 0.5 },
              { transform: 'scaleX(1)', opacity: 1 },
            ], { delay: 100 + index * 80, duration: 700, fill: 'backwards' });
          });
        }
      }
    }, { threshold: 0, rootMargin: '0px 0px -48px 0px' });

    document.querySelectorAll('.database-section, .benchmark, .intro, .feature, .utility-features, .open-source-inner, .faq, .download, .footer').forEach(section => {
      if (section.getBoundingClientRect().top >= window.innerHeight) observer.observe(section);
    });
  }

  motion.addEventListener('change', event => {
    if (event.matches) { observer?.disconnect(); finishMotion(); }
  });
  document.addEventListener('visibilitychange', () => { if (document.hidden) finishMotion(); });
  window.addEventListener('pagehide', finishMotion);
  document.addEventListener('focusin', event => {
    for (const [element, animation] of active) {
      if (element.contains(event.target)) animation.cancel();
    }
  });
}
setupPageTransitions();
