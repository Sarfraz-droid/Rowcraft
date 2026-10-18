'use strict';
document.querySelector('#copy-build').addEventListener('click', async event => { const button = event.currentTarget; try { await navigator.clipboard.writeText(document.querySelector('#copy-build').previousElementSibling.textContent); button.textContent = 'Copied!'; setTimeout(() => button.textContent = 'Copy commands', 2000); } catch { button.textContent = 'Select the commands to copy'; } });
const config = window.ROWCRAFT_CONFIG || {};
function verifiedUrl(value) { try {const url = new URL(value); return url.protocol === 'https:' ? url.href : null;} catch {return null;} }
const repo = verifiedUrl(config.repositoryUrl);
if (repo) { const github = document.querySelector('#github-link'); github.href = repo; github.target = '_blank'; github.rel = 'noopener noreferrer'; const source = document.querySelector('#source-link');source.href=repo;source.target='_blank';source.rel='noopener noreferrer'; const note = document.querySelector('#source-note'); const readme = document.createElement('a');readme.href=`${repo.replace(/\/$/,'')}`;readme.textContent='Read setup instructions in the source repository →';note.replaceChildren(readme); }
document.querySelectorAll('a[href="#source-guide"]').forEach(link => link.addEventListener('click', () => document.querySelector('#source-guide').open = true));
if (location.hash === '#source-guide') document.querySelector('#source-guide').open = true;
document.querySelector('#year').textContent = new Date().getFullYear();

// Content stays visible without JavaScript, browser animation support, or motion consent.
function setupScrollTransitions() {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  if (motion.matches || !('IntersectionObserver' in window) || !Element.prototype.animate) return;

  const animations = new Map();
  const observer = new IntersectionObserver(entries => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      observer.unobserve(entry.target);
      if (motion.matches) continue;
      const animation = entry.target.animate([
        { opacity: 0.65, transform: 'translateY(12px)' },
        { opacity: 1, transform: 'translateY(0)' },
      ], { duration: 400, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' });
      animations.set(entry.target, animation);
      animation.finished.then(() => animations.delete(entry.target), () => animations.delete(entry.target));
    }
  }, { threshold: 0, rootMargin: '0px 0px -32px 0px' });

  document.querySelectorAll('.database-section, .intro, .feature, .utility-features article, .open-source-inner, .faq, .download, .footer').forEach(section => {
    // Preserve the opening viewport and restored scroll position on refresh.
    if (section.getBoundingClientRect().top >= window.innerHeight) observer.observe(section);
  });

  function finishMotion() {
    for (const animation of animations.values()) animation.cancel();
    animations.clear();
  }
  motion.addEventListener('change', event => {
    if (event.matches) { observer.disconnect(); finishMotion(); }
  });
  document.addEventListener('focusin', event => {
    // Keyboard and anchor navigation can always reach fully visible controls.
    for (const [section, animation] of animations) {
      if (section.contains(event.target)) animation.cancel();
    }
  });
}
setupScrollTransitions();
