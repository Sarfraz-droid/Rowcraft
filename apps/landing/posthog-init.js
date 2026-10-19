'use strict';

const posthogConfig = window.ROWCRAFT_CONFIG?.posthog;
const projectToken = posthogConfig?.projectToken;
const host = posthogConfig?.host;
const isDevelopment = ['localhost', '127.0.0.1'].includes(window.location.hostname);
const pending = [];

window.RowcraftAnalytics = {
  capture: (...args) => {
    if (!projectToken || !host) return;
    if (window.posthog?.capture) window.posthog.capture(...args);
    else pending.push(['capture', args]);
  },
  logInfo: (...args) => {
    if (!projectToken || !host) return;
    if (window.posthog?.logger?.info) window.posthog.logger.info(...args);
    else pending.push(['logInfo', args]);
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
    for (const [method, args] of pending.splice(0)) {
      if (method === 'capture') window.posthog.capture(...args);
      else window.posthog.logger?.info?.(...args);
    }
  });
  document.head.appendChild(script);
}
