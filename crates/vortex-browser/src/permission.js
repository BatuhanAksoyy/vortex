// Runs in the page's own world, before its scripts, where the browser's
// prompt would sit: a call that needs the user's leave waits until Vortex
// asked and set the permission, then goes on as the page made it.
(() => {
  const send = globalThis.vortexPermission;
  if (typeof send !== 'function') return;
  delete globalThis.vortexPermission;

  const waiting = new Map();
  let lastAsk = 0;
  const ask = (permissions) => new Promise((resolve) => {
    lastAsk += 1;
    waiting.set(lastAsk, resolve);
    send(JSON.stringify({ ask: { id: lastAsk, permissions } }));
  });
  Object.defineProperty(globalThis, 'vortexPermitted', {
    value: (id) => {
      const resolve = waiting.get(id);
      waiting.delete(id);
      if (resolve) resolve();
    },
  });

  const guard = (owner, name, needs) => {
    const original = owner && owner[name];
    if (typeof original !== 'function') return;
    owner[name] = function (...args) {
      const permissions = needs(...args);
      if (permissions.length === 0) return original.apply(this, args);
      return ask(permissions).then(() => original.apply(this, args));
    };
  };

  guard(globalThis.MediaDevices?.prototype, 'getUserMedia', (constraints) =>
    [constraints?.video && 'camera', constraints?.audio && 'microphone'].filter(Boolean));
  guard(globalThis.Navigator?.prototype, 'requestMIDIAccess', (options) =>
    [options?.sysex ? 'midiSysex' : 'midi']);

  // `watchPosition` answers with its id at once, before the user does.
  const geolocation = globalThis.Geolocation?.prototype;
  if (geolocation) {
    const { getCurrentPosition, watchPosition, clearWatch } = geolocation;
    const watches = new Map();
    let lastWatch = 0;
    geolocation.getCurrentPosition = function (...args) {
      ask(['geolocation']).then(() => getCurrentPosition.apply(this, args));
    };
    geolocation.watchPosition = function (...args) {
      lastWatch += 1;
      const id = lastWatch;
      watches.set(id, null);
      ask(['geolocation']).then(() => {
        if (watches.has(id)) watches.set(id, watchPosition.apply(this, args));
      });
      return id;
    };
    geolocation.clearWatch = function (id) {
      const watch = watches.get(id);
      watches.delete(id);
      if (watch !== null && watch !== undefined) clearWatch.call(this, watch);
    };
  }

  // A browser without a window shows no notification, so Vortex does.
  const show = (title, options) => send(JSON.stringify({
    notify: { title: String(title ?? ''), body: String(options?.body ?? '') },
  }));
  const Notification = globalThis.Notification;
  if (typeof Notification === 'function') {
    guard(Notification, 'requestPermission', () => ['notifications']);
    globalThis.Notification = new Proxy(Notification, {
      construct(target, args, newTarget) {
        const made = Reflect.construct(target, args, newTarget);
        if (target.permission === 'granted') show(...args);
        return made;
      },
    });
  }
  const registration = globalThis.ServiceWorkerRegistration?.prototype;
  const showNotification = registration?.showNotification;
  if (typeof showNotification === 'function') {
    registration.showNotification = function (title, options) {
      if (globalThis.Notification.permission === 'granted') show(title, options);
      return showNotification.call(this, title, options);
    };
  }
})();
