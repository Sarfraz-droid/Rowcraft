const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { runInNewContext } = require('node:vm');

function setup() {
  function element(id, attributes = {}) {
    return {
      id, attributes, events: {}, hidden: false,
      getAttribute(name) { return this.attributes[name]; },
      setAttribute(name, value) { this.attributes[name] = value; },
      addEventListener(name, callback) { this.events[name] = callback; },
      focus() { this.focused = true; },
    };
  }
  const panels = [element('memory-panel'), element('cpu-panel')];
  const tabs = panels.map((panel, index) => element(`tab-${index}`, { 'aria-controls': panel.id }));
  const tablist = { hidden: true, querySelectorAll: () => tabs };
  const document = {
    querySelector: () => tablist,
    getElementById: id => panels.find(panel => panel.id === id),
  };
  runInNewContext(readFileSync(`${__dirname}/../benchmark-tabs.js`, 'utf8'), { document });
  return { panels, tabs, tablist };
}

test('memory is selected initially and clicking CPU switches the visible panel', () => {
  const { panels, tabs, tablist } = setup();
  assert.equal(tablist.hidden, false);
  assert.equal(panels[0].hidden, false);
  assert.equal(panels[1].hidden, true);
  tabs[1].events.click();
  assert.equal(panels[0].hidden, true);
  assert.equal(panels[1].hidden, false);
  assert.equal(tabs[0].getAttribute('aria-selected'), 'false');
  assert.equal(tabs[1].getAttribute('aria-selected'), 'true');
  assert.equal(tabs[0].tabIndex, -1);
  assert.equal(tabs[1].tabIndex, 0);
  assert.equal(panels[1].getAttribute('aria-labelledby'), tabs[1].id);
});

test('arrow keys wrap and Home/End move selection and keyboard focus', () => {
  const { panels, tabs } = setup();
  for (const [from, key, to] of [[0, 'ArrowLeft', 1], [1, 'ArrowRight', 0], [0, 'End', 1], [1, 'Home', 0]]) {
    let prevented = false;
    tabs[to].focused = false;
    tabs[from].events.keydown({ key, preventDefault() { prevented = true; } });
    assert.equal(prevented, true);
    assert.equal(tabs[to].focused, true);
    assert.equal(panels[to].hidden, false);
    assert.equal(panels[1 - to].hidden, true);
  }
});

test('memory and CPU bars agree with the recorded Rowcraft averages and peaks', () => {
  const html = readFileSync(`${__dirname}/../index.html`, 'utf8');
  const { summary: { Rowcraft: metrics } } = JSON.parse(readFileSync(`${__dirname}/../assets/benchmarks/connected-idle.json`, 'utf8'));
  for (const [metric, scale, average, peak] of [
    ['memory', 200, metrics.average_rss_mib, metrics.peak_rss_mib],
    ['cpu', 4, metrics.average_cpu_percent, metrics.peak_cpu_percent],
  ]) {
    const panel = html.split(`id="${metric}-panel"`)[1].split('</figure>')[0];
    const averageWidth = Number(panel.match(/class="chart-rowcraft"[^>]*width="([\d.]+)"/)[1]);
    const peakWidth = Number(panel.match(/class="chart-peak"[^>]*width="([\d.]+)"/)[1]);
    assert.ok(Math.abs(averageWidth - average / scale * 500) < 0.001);
    assert.ok(Math.abs(peakWidth - peak / scale * 500) < 0.001);
  }
});
