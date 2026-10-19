import { describe, expect, it } from 'vitest';
import { zoomCamera, fitCamera, MIN_MAP_ZOOM, MAX_MAP_ZOOM } from './map-camera';

describe('map camera', () => {
  it('keeps the world point beneath a pinch anchor stationary', () => {
    const before = { x: -100, y: 50, zoom: .75 }; const point = { x: 420, y: 200 };
    const after = zoomCamera(before, 1.8, point);
    expect((point.x - before.x) / before.zoom).toBeCloseTo((point.x - after.x) / after.zoom);
    expect((point.y - before.y) / before.zoom).toBeCloseTo((point.y - after.y) / after.zoom);
  });
  it('bounds gesture zoom and centers a fitted diagram', () => {
    expect(zoomCamera({ x: 0, y: 0, zoom: 1 }, 20, { x: 0, y: 0 }).zoom).toBe(MAX_MAP_ZOOM);
    expect(zoomCamera({ x: 0, y: 0, zoom: 1 }, .001, { x: 0, y: 0 }).zoom).toBe(MIN_MAP_ZOOM);
    const camera = fitCamera(2000, 1000, 1048, 548);
    expect(camera.zoom).toBe(.5); expect(camera.x).toBe(24); expect(camera.y).toBe(24);
  });
});
