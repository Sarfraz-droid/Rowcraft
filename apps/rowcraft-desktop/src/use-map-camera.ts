import { useCallback, useEffect, useRef, useState } from 'react';
import { clampZoom, fitCamera, zoomCamera, type Camera, type Point } from './map-camera';

type SafariGesture = Event & { scale: number; clientX?: number; clientY?: number };
export function useMapCamera(width: number, height: number) {
  const viewport = useRef<HTMLDivElement>(null);
  const layer = useRef<HTMLDivElement>(null);
  const camera = useRef<Camera>({ x: 24, y: 24, zoom: 1 });
  const fitted = useRef(false);
  const frame = useRef<number | null>(null);
  const [percent, setPercent] = useState(100);
  const paint = useCallback((next: Camera) => {
    camera.current = next;
    if (frame.current !== null) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = null;
      const value = camera.current;
      if (layer.current) layer.current.style.transform = `translate(${value.x}px, ${value.y}px) scale(${value.zoom})`;
      setPercent(Math.round(value.zoom * 100));
    });
  }, []);
  const fit = useCallback(() => {
    const bounds = viewport.current?.getBoundingClientRect();
    if (!bounds?.width || !bounds.height || !width || !height) return;
    fitted.current = true; paint(fitCamera(width, height, bounds.width, bounds.height));
  }, [width, height, paint]);
  const zoom = useCallback((factor: number) => {
    const bounds = viewport.current?.getBoundingClientRect();
    if (!bounds) return;
    paint(zoomCamera(camera.current, camera.current.zoom * factor, { x: bounds.width / 2, y: bounds.height / 2 }));
  }, [paint]);
  const actualSize = useCallback(() => {
    const bounds = viewport.current?.getBoundingClientRect();
    if (bounds) paint(zoomCamera(camera.current, 1, { x: bounds.width / 2, y: bounds.height / 2 }));
  }, [paint]);
  useEffect(() => { fitted.current = false; fit(); }, [fit]);
  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => { if (!fitted.current) fit(); });
    observer?.observe(element);
    return () => observer?.disconnect();
  }, [fit]);
  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    let gestureZoom: number | null = null;
    const position = (clientX?: number, clientY?: number): Point => {
      const bounds = element.getBoundingClientRect();
      return { x: clientX === undefined ? bounds.width / 2 : clientX - bounds.left, y: clientY === undefined ? bounds.height / 2 : clientY - bounds.top };
    };
    const wheel = (event: WheelEvent) => {
      if (event.cancelable) event.preventDefault();
      if (gestureZoom !== null && (event.ctrlKey || event.metaKey)) return;
      const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? element.clientHeight || 800 : 1;
      if (event.ctrlKey || event.metaKey) {
        paint(zoomCamera(camera.current, camera.current.zoom * Math.exp(-event.deltaY * unit * .008), position(event.clientX, event.clientY)));
      } else paint({ ...camera.current, x: camera.current.x - event.deltaX * unit, y: camera.current.y - event.deltaY * unit });
    };
    const startGesture = (event: Event) => { event.preventDefault(); gestureZoom = camera.current.zoom; };
    const changeGesture = (event: Event) => {
      event.preventDefault(); const gesture = event as SafariGesture;
      if (gestureZoom !== null && Number.isFinite(gesture.scale) && gesture.scale > 0) {
        paint(zoomCamera(camera.current, gestureZoom * gesture.scale, position(gesture.clientX, gesture.clientY)));
      }
    };
    const endGesture = (event: Event) => { event.preventDefault(); gestureZoom = null; };
    const pointers = new Map<number, Point>();
    let drag: { camera: Camera; points: Point[] } | null = null;
    const resetDrag = () => { drag = { camera: { ...camera.current }, points: [...pointers.values()] }; };
    const pointerDown = (event: PointerEvent) => {
      if (event.button !== 0 || (event.target as Element).closest('button, input, select, a')) return;
      pointers.set(event.pointerId, position(event.clientX, event.clientY));
      element.setPointerCapture(event.pointerId); resetDrag(); element.classList.add('is-panning');
    };
    const pointerMove = (event: PointerEvent) => {
      if (!pointers.has(event.pointerId) || !drag) return;
      pointers.set(event.pointerId, position(event.clientX, event.clientY));
      const points = [...pointers.values()];
      if (points.length === 1 && drag.points.length === 1) {
        paint({ ...drag.camera, x: drag.camera.x + points[0]!.x - drag.points[0]!.x, y: drag.camera.y + points[0]!.y - drag.points[0]!.y });
      } else if (points.length >= 2 && drag.points.length >= 2) {
        const center = (values: Point[]) => ({ x: (values[0]!.x + values[1]!.x) / 2, y: (values[0]!.y + values[1]!.y) / 2 });
        const distance = (values: Point[]) => Math.hypot(values[0]!.x - values[1]!.x, values[0]!.y - values[1]!.y);
        const start = center(drag.points); const current = center(points);
        const next = zoomCamera(drag.camera, clampZoom(drag.camera.zoom * distance(points) / Math.max(1, distance(drag.points))), start);
        paint({ ...next, x: next.x + current.x - start.x, y: next.y + current.y - start.y });
      }
    };
    const pointerUp = (event: PointerEvent) => {
      pointers.delete(event.pointerId);
      if (pointers.size) resetDrag(); else { drag = null; element.classList.remove('is-panning'); }
    };
    element.addEventListener('wheel', wheel, { passive: false });
    element.addEventListener('gesturestart', startGesture, { passive: false });
    element.addEventListener('gesturechange', changeGesture, { passive: false });
    element.addEventListener('gestureend', endGesture, { passive: false });
    element.addEventListener('pointerdown', pointerDown); element.addEventListener('pointermove', pointerMove);
    element.addEventListener('pointerup', pointerUp); element.addEventListener('pointercancel', pointerUp); element.addEventListener('lostpointercapture', pointerUp);
    return () => {
      element.removeEventListener('wheel', wheel); element.removeEventListener('gesturestart', startGesture);
      element.removeEventListener('gesturechange', changeGesture); element.removeEventListener('gestureend', endGesture);
      element.removeEventListener('pointerdown', pointerDown); element.removeEventListener('pointermove', pointerMove);
      element.removeEventListener('pointerup', pointerUp); element.removeEventListener('pointercancel', pointerUp); element.removeEventListener('lostpointercapture', pointerUp);
      if (frame.current !== null) { cancelAnimationFrame(frame.current); frame.current = null; }
    };
  }, [paint]);
  return { viewport, layer, percent, fit, zoom, actualSize };
}
