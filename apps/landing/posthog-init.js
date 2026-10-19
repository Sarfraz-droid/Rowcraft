'use strict';

const posthogConfig = window.ROWCRAFT_CONFIG?.posthog;
const projectToken = posthogConfig?.projectToken;
const host = posthogConfig?.host;
const isDevelopment = ['localhost', '127.0.0.1', '[::1]'].includes(window.location.hostname);
const pending = [];
let ready = false;

window.RowcraftAnalytics = {
  capture: (...args) => {
    if (!projectToken || !host) return;
    if (ready && window.posthog?.capture) window.posthog.capture(...args);
    else if (pending.length < 100) pending.push(['capture', args]);
  },
  logInfo: (...args) => {
    if (!projectToken || !host) return;
    if (ready && window.posthog?.logger?.info) window.posthog.logger.info(...args);
    else if (pending.length < 100) pending.push(['logInfo', args]);
  },
};

if (!projectToken || !host) {
  if (isDevelopment) {
    const missingVariable = projectToken ? 'POSTHOG_HOST' : 'POSTHOG_PROJECT_TOKEN';
    console.error(`${missingVariable} variable required by PostHog is missing or un-configured, this causes events to be silently missed. This error stops appearing once ${missingVariable} is configured`);
  }
} else {
  const script = document.createElement('script');
  script.async = true;
  script.src = `${host.replace('.i.posthog.com', '-assets.i.posthog.com').replace(/\/$/, '')}/static/array.js`;
  script.addEventListener('load', () => {
    window.posthog.init(projectToken, {
      api_host: host,
      // Emit exactly once below, before actions queued while the SDK loads.
      // The SDK's automatic pageview runs on a timer after `loaded`.
      capture_pageview: false,
      capture_pageleave: true,
      autocapture: false,
      person_profiles: 'identified_only',
      loaded: posthog => {
        posthog.register({ app: 'rowcraft-landing', environment: isDevelopment ? 'development' : 'production' });
        posthog.capture('$pageview');
        ready = true;
        for (const [method, args] of pending.splice(0)) {
          if (method === 'capture') posthog.capture(...args);
          else posthog.logger?.info?.(...args);
        }
      },
      capture_exceptions: {
        capture_unhandled_errors: true,
        capture_unhandled_rejections: true,
        capture_console_errors: false,
      },
      logs: {
        serviceName: 'rowcraft-landing',
        environment: isDevelopment ? 'development' : 'production',
      },
    });
  });
  script.addEventListener('error', () => { pending.length = 0; });
  document.head.appendChild(script);
}
