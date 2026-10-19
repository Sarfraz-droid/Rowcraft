const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { runInNewContext } = require('node:vm');

function setup({ configured = true, hostname = 'rowcraft.example' } = {}) {
  const events = [], scripts = [], props = {};
  const sdk = {
    register: values => Object.assign(props, values),
    capture: (name, values, options) => events.push({ name, values: { ...props, ...values }, options }),
    init: (token, config) => { sdk.config = config; config.loaded(sdk); },
  };
  const window = { location: { hostname }, ROWCRAFT_CONFIG: { posthog: configured ? { projectToken: 'phc_example', host: 'https://us.i.posthog.com' } : {} } };
  const document = {
    createElement: () => ({ events: {}, addEventListener(name, callback) { this.events[name] = callback; } }),
    head: { appendChild: script => scripts.push(script) },
  };
  runInNewContext(readFileSync(`${__dirname}/../posthog-init.js`, 'utf8'), { window, document, console });
  return { window, events, scripts, sdk, load() { window.posthog = sdk; scripts[0].events.load(); } };
}

test('pageview precedes queued conversion events, captures once, and forwards beacon options', () => {
  const app = setup();
  app.window.RowcraftAnalytics.capture('download_cta_clicked', { surface: 'hero' });
  app.load();
  app.window.RowcraftAnalytics.capture('installer_download_started', { platform: 'macos' }, { transport: 'sendBeacon', send_instantly: true });
  assert.deepEqual(app.events.map(e => e.name), ['$pageview', 'download_cta_clicked', 'installer_download_started']);
  assert.equal(app.events[0].values.app, 'rowcraft-landing');
  assert.equal(app.events[0].values.environment, 'production');
  assert.equal(app.sdk.config.capture_pageview, false);
  assert.equal(app.sdk.config.autocapture, false);
  assert.equal(app.events[2].options.transport, 'sendBeacon');
});

test('local traffic is marked development and missing config never loads a script', () => {
  const local = setup({ hostname: '127.0.0.1' });
  local.load();
  assert.equal(local.events[0].values.environment, 'development');
  const disabled = setup({ configured: false });
  disabled.window.RowcraftAnalytics.capture('download_cta_clicked');
  assert.equal(disabled.scripts.length, 0);
  assert.equal(disabled.events.length, 0);
});

test('SDK failure leaves analytics calls harmless', () => {
  const app = setup();
  app.scripts[0].events.error();
  assert.doesNotThrow(() => app.window.RowcraftAnalytics.capture('download_cta_clicked'));
  assert.equal(app.events.length, 0);
});

test('CTA placement, section visibility, and details actions have stable payloads', () => {
  const events = [], handlers = {}, details = { open: false, id: 'source-guide', addEventListener: (name, fn) => { handlers.toggle = fn; } };
  let observe;
  let disconnected = false;
  class Observer {
    constructor(callback) { observe = callback; }
    observe() {}
    disconnect() { disconnected = true; }
  }
  const window = { RowcraftAnalytics: { capture: (...args) => events.push(args) }, IntersectionObserver: Observer };
  const document = {
    addEventListener: (name, fn) => { handlers[name] = fn; },
    querySelectorAll: () => [details], querySelector: () => ({}),
  };
  runInNewContext(readFileSync(`${__dirname}/../analytics.js`, 'utf8'), { window, document, IntersectionObserver: Observer });
  for (const placement of ['header', 'hero', 'footer', 'content']) {
    const link = { id: '', getAttribute: () => '#download', closest: selector => selector === ({ header: 'header', hero: '.hero', footer: 'footer' }[placement]) };
    handlers.click({ target: { closest: () => link } });
    assert.equal(events.at(-1)[0], 'download_cta_clicked');
    assert.equal(events.at(-1)[1].surface, placement);
  }
  observe([{ isIntersecting: false }]);
  assert.equal(disconnected, false);
  observe([{ isIntersecting: true }]);
  assert.equal(events.at(-1)[0], 'download_section_viewed');
  assert.equal(disconnected, true);
  details.open = true;
  handlers.toggle();
  assert.equal(events.at(-1)[0], 'source_guide_opened');
  const count = events.length;
  details.open = false;
  handlers.toggle();
  assert.equal(events.length, count);
});
