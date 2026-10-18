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

test('CPU index and bar widths agree with the measured averages', () => {
  const html = readFileSync(`${__dirname}/../index.html`, 'utf8');
  const { summary } = JSON.parse(readFileSync(`${__dirname}/../assets/benchmarks/connected-idle.json`, 'utf8'));
  const cpuPanel = html.split('id="cpu-panel"')[1].split('</figure>')[0];
  const ratio = summary.Rowcraft.average_cpu_percent / summary.DataGrip.average_cpu_percent;
  const width = Number(cpuPanel.match(/class="chart-rowcraft"[^>]*width="([\d.]+)"/)[1]);
  assert.ok(Math.abs(width - ratio * 500) < 0.001);
  assert.ok(cpuPanel.includes(`>${(ratio * 100).toFixed(1)}</text>`));
  assert.ok(cpuPanel.includes('>100</text>'));
});
