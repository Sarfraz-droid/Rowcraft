export interface Camera { x: number; y: number; zoom: number }
export interface Point { x: number; y: number }
export const MIN_MAP_ZOOM = .15;
export const MAX_MAP_ZOOM = 2.5;
export function clampZoom(zoom: number) { return Math.max(MIN_MAP_ZOOM, Math.min(MAX_MAP_ZOOM, zoom)); }
/** Preserve the world position under the gesture center. */
export function zoomCamera(camera: Camera, zoom: number, anchor: Point): Camera {
  const next = clampZoom(zoom);
  const ratio = next / camera.zoom;
  return { x: anchor.x - (anchor.x - camera.x) * ratio, y: anchor.y - (anchor.y - camera.y) * ratio, zoom: next };
}
export function fitCamera(width: number, height: number, viewportWidth: number, viewportHeight: number): Camera {
  const zoom = clampZoom(Math.min(1, (viewportWidth - 48) / Math.max(width, 1), (viewportHeight - 48) / Math.max(height, 1)));
  return { x: (viewportWidth - width * zoom) / 2, y: (viewportHeight - height * zoom) / 2, zoom };
}
