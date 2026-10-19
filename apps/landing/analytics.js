/* Stable action names; never collect link text, commands, or user-entered values. */
(() => {
  const capture = (name, properties) => window.RowcraftAnalytics?.capture(name, properties);
  function surface(element) {
    if (element.closest('header')) return 'header';
    if (element.closest('.hero')) return 'hero';
    if (element.closest('footer')) return 'footer';
    return 'content';
  }
  document.addEventListener('click', event => {
    const link = event.target.closest('a');
    if (!link) return;
    const href = link.getAttribute('href');
    if (href === '#download') capture('download_cta_clicked', { surface: surface(link) });
    if (['github-link', 'source-link'].includes(link.id) || link.closest('#source-note')) {
      capture('repository_link_clicked', { surface: surface(link) });
    }
  });
  document.querySelectorAll('.faq details, #source-guide').forEach((details, index) => {
    details.addEventListener('toggle', () => {
      if (details.open) capture(details.id === 'source-guide' ? 'source_guide_opened' : 'faq_opened', { item_index: index });
    });
  });
  // A visitor can reach downloads directly, without clicking a CTA or changing OS.
  const section = document.querySelector('#download');
  if (section && 'IntersectionObserver' in window) {
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return;
      capture('download_section_viewed');
      observer.disconnect();
    }, { threshold: 0.1 });
    observer.observe(section);
  }
})();
