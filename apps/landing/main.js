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
        { opacity: 0.15, transform: 'translateY(28px)' },
        { opacity: 1, transform: 'translateY(0)' },
      ], { duration: 750, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' });
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

// Only the recessed frame follows the pointer; the screenshot stays on a crisp 2D plane.
function setupPreviewTilt() {
  const stage = document.querySelector('.preview-stage');
  const shot = document.querySelector('.product-shot');
  const depth = document.querySelector('.preview-depth');
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const pointer = window.matchMedia('(hover: hover) and (pointer: fine)');
  if (!stage || !shot || !depth) return;

  let frame = 0;
  let currentX = 0;
  let currentY = 0;
  let targetX = 0;
  let targetY = 0;

  function update() {
    currentX += (targetX - currentX) * 0.12;
    currentY += (targetY - currentY) * 0.12;
    depth.style.transform = `rotateX(${currentX.toFixed(3)}deg) rotateY(${currentY.toFixed(3)}deg)`;
    if (Math.abs(targetX - currentX) + Math.abs(targetY - currentY) > 0.01) {
      frame = requestAnimationFrame(update);
    } else {
      frame = 0;
      if (targetX === 0 && targetY === 0) {
        depth.style.removeProperty('transform');
        stage.classList.remove('is-tilting');
      }
    }
  }
  function schedule() {
    if (!frame) frame = requestAnimationFrame(update);
  }
  function reset(immediate = false) {
    targetX = targetY = 0;
    if (immediate) {
      cancelAnimationFrame(frame);
      frame = currentX = currentY = 0;
      depth.style.removeProperty('transform');
      stage.classList.remove('is-tilting');
    } else schedule();
  }
  stage.addEventListener('pointermove', event => {
    if (motion.matches || !pointer.matches || event.pointerType === 'touch' || shot.matches(':focus-visible')) return;
    const bounds = stage.getBoundingClientRect();
    const x = Math.max(-0.5, Math.min(0.5, (event.clientX - bounds.left) / bounds.width - 0.5));
    const y = Math.max(-0.5, Math.min(0.5, (event.clientY - bounds.top) / bounds.height - 0.5));
    targetX = -y * 5;
    targetY = x * 6;
    stage.classList.add('is-tilting');
    schedule();
  }, { passive: true });
  stage.addEventListener('pointerleave', () => reset());
  stage.addEventListener('pointercancel', () => reset(true));
  shot.addEventListener('focus', () => reset(true));
  window.addEventListener('blur', () => reset(true));
  window.addEventListener('pagehide', () => reset(true));
  document.addEventListener('visibilitychange', () => { if (document.hidden) reset(true); });
  motion.addEventListener('change', () => reset(true));
  pointer.addEventListener('change', () => reset(true));
}
setupPreviewTilt();
