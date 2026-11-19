(() => {
  const container = document.querySelector('#terminal');
  const status = document.querySelector('#status');
  const embedded = window.parent !== window;
  const bridge = window.nsh;
  const hostTransport = typeof bridge?.postMessage === 'function';
  const terminal = new Terminal({
    cursorBlink: true,
    disableStdin: true,
    fontFamily: "Consolas, 'IBM Plex Mono', monospace",
    fontSize: 13,
    theme: {
      background: '#102d2a',
      foreground: '#e3f2e9',
      cursor: '#9ee2bb',
      selectionBackground: '#355b51',
    },
  });
  const fit = new FitAddon.FitAddon();
  terminal.loadAddon(fit);
  terminal.open(container);
  terminal.textarea?.setAttribute('aria-label', 'Workspace shell input');

  const encoder = new TextEncoder();
  let socket;
  let started = false;
  let online = false;
  let failed = false;
  let obscured = false;
  let exited = false;
  let frame = 0;
  let sentCols;
  let sentRows;

  function send(message) {
    const payload = JSON.stringify(message);
    if (hostTransport) bridge.postMessage(payload);
    else socket.send(payload);
  }

  function reportError(message) {
    failed = true;
    status.textContent = message;
    status.hidden = false;
    terminal.options.disableStdin = true;
  }

  function receive(text) {
    const message = JSON.parse(text);
    if (message.type === 'output') {
      terminal.write(Uint8Array.from(atob(message.data), (character) => character.charCodeAt(0)));
    } else if (message.type === 'error') {
      reportError(message.message);
    } else if (message.type === 'exited' && !exited) {
      exited = true;
      status.textContent = 'Shell exited.';
      status.hidden = false;
      terminal.options.disableStdin = true;
      terminal.write('', () => bridge?.postMessage('exited'));
    }
  }

  function opened() {
    online = true;
    terminal.options.disableStdin = false;
    status.hidden = true;
    resize(terminal);
  }

  function closed() {
    online = false;
    terminal.options.disableStdin = true;
    if (!failed && !exited) {
      reportError('Shell connection closed. Close and reopen the terminal to reconnect.');
    }
  }

  function start() {
    started = true;
    const { cols, rows } = terminal;
    sentCols = cols;
    sentRows = rows;
    if (hostTransport) {
      send({ type: 'connect', cols, rows });
      return;
    }

    const workspace = new URLSearchParams(location.search).get('workspace_id');
    if (!workspace) {
      reportError('Missing workspace for this terminal.');
      return;
    }
    const url = new URL('ws', location.href);
    url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    url.search = new URLSearchParams({ workspace_id: workspace, cols, rows });
    socket = new WebSocket(url);
    socket.addEventListener('open', opened);
    socket.addEventListener('message', ({ data }) => receive(data));
    socket.addEventListener('error', () => reportError('Could not connect to the workspace shell.'));
    socket.addEventListener('close', closed);
  }

  function scheduleFit() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (obscured || !container.clientWidth || !container.clientHeight) return;
      fit.fit();
      if (!started) start();
    });
  }

  // NSH's srcdoc sandbox uses a MessagePort for outbound messages. The player
  // sends inbound messages with window.postMessage; WebView2 dispatches locally.
  window.addEventListener('message', (event) => {
    if (event.source !== (embedded ? window.parent : null) || typeof event.data !== 'string') return;
    switch (event.data) {
      case 'focus':
        obscured = false;
        terminal.focus();
        scheduleFit();
        break;
      case 'obscure':
        obscured = true;
        terminal.blur();
        break;
      case 'reveal':
        obscured = false;
        scheduleFit();
        break;
      default:
        if (!hostTransport) break;
        if (event.data === 'open') opened();
        else if (event.data === 'closed' || event.data === 'disconnected') closed();
        else if (event.data === 'failed') reportError('Could not connect to the workspace shell.');
        else if (event.data.startsWith('{')) receive(event.data);
        break;
    }
  });

  function input(bytes) {
    if (!online || failed || obscured || exited) return;
    // Process large clipboard pastes without a spread over the byte array.
    let binary = '';
    for (const byte of bytes) binary += String.fromCharCode(byte);
    send({ type: 'input', data: btoa(binary) });
  }

  terminal.onData((data) => input(encoder.encode(data)));
  terminal.onBinary((data) => input(Uint8Array.from(data, (character) => character.charCodeAt(0))));
  function resize({ cols, rows }) {
    if (!online || (cols === sentCols && rows === sentRows)) return;
    sentCols = cols;
    sentRows = rows;
    send({ type: 'resize', cols, rows });
  }
  terminal.onResize(resize);

  const observer = new ResizeObserver(scheduleFit);
  observer.observe(container);
  if (bridge) bridge.postMessage('ready');
  scheduleFit();
  window.addEventListener('pagehide', () => {
    observer.disconnect();
    cancelAnimationFrame(frame);
    socket?.close();
    terminal.dispose();
  }, { once: true });
})();
