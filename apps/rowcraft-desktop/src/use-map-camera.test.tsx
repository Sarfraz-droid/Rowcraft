// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useMapCamera } from './use-map-camera';

function Harness() {
  const { viewport, layer, percent } = useMapCamera(1000, 600);
  return <><div ref={viewport} aria-label="map"><div ref={layer} data-testid="layer" /></div><output>{percent}%</output></>;
}
// happy-dom's WheelEvent extends UIEvent and omits MouseEvent fields.
function wheelEvent(init: WheelEventInit) {
  return Object.assign(new WheelEvent('wheel', init), {
    ctrlKey: init.ctrlKey ?? false,
    metaKey: init.metaKey ?? false,
    clientX: init.clientX ?? 0,
    clientY: init.clientY ?? 0,
  });
}
afterEach(cleanup);
describe('map input gestures', () => {
  it.each(['ctrlKey', 'metaKey'] as const)('uses a cancellable %s trackpad pinch to zoom without browser page zoom', async (modifier) => {
    render(<Harness />);
    const event = wheelEvent({ [modifier]: true, deltaY: -50, clientX: 300, clientY: 180, cancelable: true, bubbles: true });
    fireEvent(screen.getByLabelText('map'), event);
    await screen.findByText('149%');
    expect(event.defaultPrevented).toBe(true);
    expect(screen.getByTestId('layer').style.transform).toContain('scale(1.4918');
  });
  it('pans on an ordinary two-finger scroll without changing zoom', async () => {
    render(<Harness />);
    fireEvent(screen.getByLabelText('map'), wheelEvent({ deltaX: 30, deltaY: 45, cancelable: true }));
    await waitFor(() => expect(screen.getByTestId('layer').style.transform).toBe('translate(-6px, -21px) scale(1)'));
    expect(screen.getByText('100%')).toBeTruthy();
  });
  it('supports Safari gesture events and avoids applying its simultaneous wheel pinch twice', async () => {
    render(<Harness />);
    const map = screen.getByLabelText('map');
    fireEvent(map, new Event('gesturestart', { cancelable: true }));
    const wheel = wheelEvent({ ctrlKey: true, deltaY: -50, cancelable: true });
    fireEvent(map, wheel);
    expect(wheel.defaultPrevented).toBe(true);
    const gesture = Object.assign(new Event('gesturechange', { cancelable: true }), { scale: 1.5, clientX: 100, clientY: 80 });
    fireEvent(map, gesture);
    await screen.findByText('150%');
    expect(screen.getByTestId('layer').style.transform).toBe('translate(-14px, -4px) scale(1.5)');
    expect(gesture.defaultPrevented).toBe(true);
    fireEvent(map, new Event('gestureend', { cancelable: true }));
  });
});
