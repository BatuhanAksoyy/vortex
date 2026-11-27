// Runs in the page's own world, before its scripts. WebUSB, WebHID and Web
// Serial reach no device in a browser without a window, so these stand in
// for them: the page calls them as it would the browser's, and Vortex asks
// the user, opens the device and carries out each call.
(() => {
  const send = globalThis.vortexNative;
  if (typeof send !== 'function') return;
  delete globalThis.vortexNative;
  if (!globalThis.isSecureContext) return;

  const waiting = new Map();
  let lastCall = 0;
  const call = (kind, method, device, args) => new Promise((resolve, reject) => {
    lastCall += 1;
    waiting.set(lastCall, { resolve, reject });
    send(JSON.stringify({ id: lastCall, kind, method, device, args: args ?? {} }));
  });
  const failure = (name, message) =>
    name === 'TypeError' ? new TypeError(message) : new DOMException(message, name);
  Object.defineProperty(globalThis, 'vortexNativeReply', {
    value: (id, error, value) => {
      const pending = waiting.get(id);
      waiting.delete(id);
      if (!pending) return;
      if (error) pending.reject(failure(error.name, error.message));
      else pending.resolve(value);
    },
  });

  const toBase64 = (source) => {
    if (source === undefined || source === null) return '';
    const bytes = ArrayBuffer.isView(source)
      ? new Uint8Array(source.buffer, source.byteOffset, source.byteLength)
      : new Uint8Array(source);
    let text = '';
    for (let at = 0; at < bytes.length; at += 0x8000) {
      text += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
    }
    return btoa(text);
  };
  const fromBase64 = (text) => Uint8Array.from(atob(text ?? ''), (c) => c.charCodeAt(0));
  const view = (text) => new DataView(fromBase64(text).buffer);
  const handler = (target, type) => {
    let current = null;
    Object.defineProperty(target, `on${type}`, {
      get: () => current,
      set: (value) => {
        if (current) target.removeEventListener(type, current);
        current = typeof value === 'function' ? value : null;
        if (current) target.addEventListener(type, current);
      },
    });
  };
  const fire = (target, type, fields) => {
    const event = new Event(type);
    for (const [key, value] of Object.entries(fields)) {
      Object.defineProperty(event, key, { value });
    }
    target.dispatchEvent(event);
  };

  // One object per device for the document, as the browser keeps them.
  const known = { usb: new Map(), hid: new Map(), serial: new Map() };
  const wrap = (kind, make) => (info) => {
    const found = known[kind].get(info.id);
    if (found) return found;
    const made = make(info);
    known[kind].set(info.id, made);
    return made;
  };

  class USBDevice {
    #id;
    #configurationValue;
    #claimed = new Map();
    constructor(info) {
      this.#id = info.id;
      this.#configurationValue = info.configurationValue ?? null;
      for (const key of Object.keys(info)) {
        if (!['id', 'configurations', 'configurationValue'].includes(key)) this[key] = info[key];
      }
      this.opened = false;
      this.configurations = (info.configurations ?? []).map((configuration) => ({
        ...configuration,
        interfaces: configuration.interfaces.map((face) => this.#face(face)),
      }));
    }
    #face(face) {
      const device = this;
      return {
        interfaceNumber: face.interfaceNumber,
        alternates: face.alternates,
        get claimed() { return device.#claimed.has(face.interfaceNumber); },
        get alternate() {
          const setting = device.#claimed.get(face.interfaceNumber) ?? 0;
          return face.alternates.find((it) => it.alternateSetting === setting) ?? face.alternates[0];
        },
      };
    }
    get configuration() {
      return this.configurations.find((it) => it.configurationValue === this.#configurationValue) ?? null;
    }
    #call(method, args) { return call('usb', method, this.#id, args); }
    async open() {
      const state = await this.#call('open');
      this.#configurationValue = state?.configurationValue ?? this.#configurationValue;
      this.opened = true;
    }
    async close() {
      await this.#call('close');
      this.opened = false;
      this.#claimed.clear();
    }
    async forget() { await this.#call('forget'); this.opened = false; this.#claimed.clear(); }
    async selectConfiguration(configurationValue) {
      const state = await this.#call('selectConfiguration', { configurationValue });
      this.#configurationValue = state?.configurationValue ?? configurationValue;
      this.#claimed.clear();
    }
    async claimInterface(interfaceNumber) {
      await this.#call('claimInterface', { interfaceNumber });
      this.#claimed.set(interfaceNumber, 0);
    }
    async releaseInterface(interfaceNumber) {
      await this.#call('releaseInterface', { interfaceNumber });
      this.#claimed.delete(interfaceNumber);
    }
    async selectAlternateInterface(interfaceNumber, alternateSetting) {
      await this.#call('selectAlternateInterface', { interfaceNumber, alternateSetting });
      this.#claimed.set(interfaceNumber, alternateSetting);
    }
    async controlTransferIn(setup, length) {
      const done = await this.#call('controlTransferIn', { setup, length });
      return { status: done.status, data: view(done.data) };
    }
    controlTransferOut(setup, data) {
      return this.#call('controlTransferOut', { setup, data: toBase64(data) });
    }
    async transferIn(endpointNumber, length) {
      const done = await this.#call('transferIn', { endpointNumber, length });
      return { status: done.status, data: view(done.data) };
    }
    transferOut(endpointNumber, data) {
      return this.#call('transferOut', { endpointNumber, data: toBase64(data) });
    }
    clearHalt(direction, endpointNumber) { return this.#call('clearHalt', { direction, endpointNumber }); }
    reset() { return this.#call('reset'); }
    isochronousTransferIn() { return Promise.reject(failure('NotSupportedError', 'Isochronous transfers are not supported.')); }
    isochronousTransferOut() { return this.isochronousTransferIn(); }
  }

  class HIDDevice extends EventTarget {
    #id;
    constructor(info) {
      super();
      this.#id = info.id;
      this.vendorId = info.vendorId;
      this.productId = info.productId;
      this.productName = info.productName;
      this.collections = info.collections;
      this.opened = false;
      handler(this, 'inputreport');
    }
    #call(method, args) { return call('hid', method, this.#id, args); }
    async open() { await this.#call('open'); this.opened = true; }
    async close() { await this.#call('close'); this.opened = false; }
    async forget() { await this.#call('forget'); this.opened = false; }
    sendReport(reportId, data) { return this.#call('sendReport', { reportId, data: toBase64(data) }); }
    sendFeatureReport(reportId, data) {
      return this.#call('sendFeatureReport', { reportId, data: toBase64(data) });
    }
    async receiveFeatureReport(reportId) { return view(await this.#call('receiveFeatureReport', { reportId })); }
    input(event) {
      if (this.opened) fire(this, 'inputreport', { device: this, reportId: event.reportId, data: view(event.data) });
    }
  }

  class SerialPort extends EventTarget {
    #id;
    #info;
    #open = false;
    #readable = null;
    #reading = null;
    #writable = null;
    constructor(info) {
      super();
      this.#id = info.id;
      this.#info = { ...info };
      delete this.#info.id;
      this.connected = true;
      handler(this, 'connect');
      handler(this, 'disconnect');
    }
    #call(method, args) { return call('serial', method, this.#id, args); }
    getInfo() { return { ...this.#info }; }
    get readable() {
      if (!this.#open) return null;
      this.#readable ??= new ReadableStream({
        start: (controller) => { this.#reading = controller; },
        cancel: () => { this.#readable = null; this.#reading = null; },
      });
      return this.#readable;
    }
    get writable() {
      if (!this.#open) return null;
      this.#writable ??= new WritableStream({
        write: (chunk) => this.#call('write', { data: toBase64(chunk) }),
        close: () => { this.#writable = null; },
        abort: () => { this.#writable = null; },
      });
      return this.#writable;
    }
    async open(options) {
      if (this.#open) throw failure('InvalidStateError', 'The port is already open.');
      await this.#call('open', options);
      this.#open = true;
    }
    async close() {
      if (this.#readable?.locked || this.#writable?.locked) {
        throw new TypeError('Cannot close a port whose streams are locked.');
      }
      await this.#readable?.cancel();
      await this.#writable?.abort();
      await this.#call('close');
      this.#open = false;
      this.#readable = this.#writable = null;
    }
    async forget() { await this.#call('forget'); this.#open = false; }
    setSignals(signals) { return this.#call('setSignals', signals); }
    getSignals() { return this.#call('getSignals'); }
    data(event) { this.#reading?.enqueue(fromBase64(event.data)); }
    lost() {
      this.#reading?.error(failure('NetworkError', 'The device has been lost.'));
      this.#readable = this.#reading = null;
    }
  }

  const usbDevice = wrap('usb', (info) => new USBDevice(info));
  const hidDevice = wrap('hid', (info) => new HIDDevice(info));
  const serialPort = wrap('serial', (info) => new SerialPort(info));

  const api = (kind, made, methods) => {
    const target = new EventTarget();
    handler(target, 'connect');
    handler(target, 'disconnect');
    Object.assign(target, methods(made));
    return target;
  };
  const filtered = (options) => {
    if (!options || !Array.isArray(options.filters)) {
      return Promise.reject(new TypeError("The 'filters' member is required."));
    }
    return null;
  };
  const usb = api('usb', usbDevice, (made) => ({
    getDevices: async () => (await call('usb', 'getDevices')).map(made),
    requestDevice: (options) =>
      filtered(options) ?? call('usb', 'requestDevice', null, { filters: options.filters }).then(made),
  }));
  const hid = api('hid', hidDevice, (made) => ({
    getDevices: async () => (await call('hid', 'getDevices')).map(made),
    requestDevice: (options) => filtered(options)
      ?? call('hid', 'requestDevice', null, { filters: options.filters })
        .then((info) => [made(info)], (error) => {
          if (error.name === 'NotFoundError') return [];
          throw error;
        }),
  }));
  const serial = api('serial', serialPort, (made) => ({
    getPorts: async () => (await call('serial', 'getPorts')).map(made),
    requestPort: (options) =>
      call('serial', 'requestPort', null, { filters: options?.filters ?? [] }).then(made),
  }));
  const apis = { usb: [usb, usbDevice], hid: [hid, hidDevice], serial: [serial, serialPort] };

  Object.defineProperty(globalThis, 'vortexNativeEvent', {
    value: (kind, id, event) => {
      const [target, made] = apis[kind] ?? [];
      if (!target) return;
      const device = event.info ? made({ ...event.info, id }) : known[kind].get(id);
      if (!device) return;
      if (event.type === 'data') device.data(event);
      else if (event.type === 'lost' && kind === 'serial') device.lost();
      else if (event.type === 'inputreport') device.input(event);
      else if (event.type === 'connect' || event.type === 'disconnect') {
        if (kind === 'serial') {
          device.connected = event.type === 'connect';
          fire(device, event.type, {});
        }
        fire(target, event.type, kind === 'serial' ? {} : { device });
      }
    },
  });

  for (const [name, target] of Object.entries({ usb, hid, serial })) {
    Object.defineProperty(Navigator.prototype, name, { get: () => target, configurable: true });
  }
})();
