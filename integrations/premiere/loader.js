/* Strata panel for Premiere Pro — loader.
 *
 * The only part installed on the editor's machine with the manifest. The
 * panel itself (ui/panel.html, panel.css, panel.js and the ExtendScript of
 * host/premiere.jsx) comes from the Strata server, so updating the server
 * updates every panel. The last copy loaded is kept for offline use; the
 * copy installed next to this file is the fallback.
 *
 * When the server has a newer loader (LOADER_VERSION below: bump it with any
 * change to this file, index.html or CSXS/manifest.xml), the panel offers
 * an update: window.STRATA_LOADER.install() writes the new files into the
 * extension folder, then the panel reloads (Premiere restarts for a new
 * manifest).
 *
 * Runs in the panel's own page (Node and Premiere's scripting stay
 * available to the code injected). CEP 9+ (Chromium 61): plain ES2015.
 */
(function () {
  'use strict';

  var LOADER_VERSION = 3;
  var UI_FILES = ['ui/panel.html', 'ui/panel.css', 'ui/panel.js', 'host/premiere.jsx'];
  var node = typeof require === 'function';
  var fs = node ? require('fs') : null;
  var path = node ? require('path') : null;

  function setting(k) {
    try {
      var v = localStorage.getItem('pcg.' + k);
      return v === null ? '' : JSON.parse(v);
    } catch (e) {
      return '';
    }
  }

  /** The extension folder on disk. */
  function extensionDir() {
    var p = decodeURIComponent(location.pathname);
    if (/^\/[A-Za-z]:\//.test(p)) p = p.slice(1); // file:///C:/…
    return path ? path.dirname(p) : '';
  }

  function readLocal(file) {
    if (!fs) return null;
    try {
      return fs.readFileSync(path.join(extensionDir(), file), 'utf8');
    } catch (e) {
      return null;
    }
  }

  function withTimeout(p, ms) {
    return new Promise(function (resolve, reject) {
      var h = setTimeout(function () {
        reject(new Error('timeout'));
      }, ms);
      p.then(
        function (v) {
          clearTimeout(h);
          resolve(v);
        },
        function (e) {
          clearTimeout(h);
          reject(e);
        },
      );
    });
  }

  function get(server, url, json) {
    return withTimeout(
      fetch(server + url, { cache: 'no-store' }).then(function (r) {
        if (!r.ok) throw new Error(r.status + ' ' + r.statusText);
        return json ? r.json() : r.text();
      }),
      6000,
    );
  }

  /** The panel files from the server: { version, loader, files: {path: text} }. */
  function fromServer(server) {
    return get(server, '/premiere/manifest.json', true).then(function (m) {
      return Promise.all(
        UI_FILES.map(function (f) {
          return get(server, '/premiere/' + f, false);
        }),
      ).then(function (texts) {
        var files = {};
        UI_FILES.forEach(function (f, i) {
          files[f] = texts[i];
        });
        return { version: m.version, loader: m.loader, files: files, from: 'server' };
      });
    });
  }

  function cached() {
    try {
      var c = JSON.parse(localStorage.getItem('pcg.uiCache') || 'null');
      return c && c.files ? Object.assign(c, { from: 'cache' }) : null;
    } catch (e) {
      return null;
    }
  }

  function bundled() {
    var files = {};
    for (var i = 0; i < UI_FILES.length; i++) {
      var t = readLocal(UI_FILES[i]);
      if (t === null) return null;
      files[UI_FILES[i]] = t;
    }
    return { version: 'installed', loader: LOADER_VERSION, files: files, from: 'installed' };
  }

  function evalHost(code) {
    return new Promise(function (resolve) {
      if (!window.__adobe_cep__ || !code) return resolve();
      window.__adobe_cep__.evalScript(code, function () {
        resolve();
      });
    });
  }

  function start(ui) {
    var style = document.createElement('style');
    style.textContent = ui.files['ui/panel.css'];
    document.head.appendChild(style);
    document.body.innerHTML = ui.files['ui/panel.html'];
    window.STRATA_LOADER = {
      version: LOADER_VERSION,
      server: server,
      ui: ui.version,
      from: ui.from,
      latest: ui.loader || LOADER_VERSION,
      updateAvailable: (ui.loader || 0) > LOADER_VERSION && ui.from === 'server',
      install: install,
    };
    // The Premiere side first (the server's copy replaces the installed one).
    return evalHost(ui.files['host/premiere.jsx']).then(function () {
      var s = document.createElement('script');
      s.textContent = ui.files['ui/panel.js'] + '\n//# sourceURL=strata-panel.js';
      document.body.appendChild(s);
    });
  }

  // CEP 9 runs Node 8: no mkdirSync({ recursive }).
  function mkdirs(dir) {
    if (fs.existsSync(dir)) return;
    mkdirs(path.dirname(dir));
    fs.mkdirSync(dir);
  }

  /** Writes the server's extension (manifest, loader, fallback copy) into the extension folder, then reloads. */
  function install() {
    var server = (setting('server') || '').replace(/\/+$/, '');
    if (!fs || !server) return Promise.reject(new Error('Cannot update: no server or no Node.js.'));
    var dir = extensionDir();
    var before = readLocal('CSXS/manifest.xml');
    return get(server, '/premiere/extension.json', true).then(function (bundle) {
      Object.keys(bundle.files).forEach(function (rel) {
        if (rel.indexOf('..') >= 0) return;
        var to = path.join(dir, rel);
        mkdirs(path.dirname(to));
        fs.writeFileSync(to, Buffer.from(bundle.files[rel], 'base64'));
      });
      localStorage.removeItem('pcg.uiCache');
      // A new manifest is read when Premiere starts.
      var restart = before !== null && before !== readLocal('CSXS/manifest.xml');
      return { restart: restart, reload: function () {
        location.reload();
      } };
    });
  }

  /** The server an installer wrote next to the loader (strata.ini: server=…), for the first start. */
  function installedServer() {
    var ini = readLocal('strata.ini') || '';
    var m = /^\s*server\s*=\s*(\S+)/m.exec(ini);
    if (!m) return '';
    try {
      localStorage.setItem('pcg.server', JSON.stringify(m[1]));
    } catch (e) {}
    return m[1];
  }

  var server = (setting('server') || installedServer()).replace(/\/+$/, '');
  var load = server
    ? fromServer(server).then(
        function (ui) {
          try {
            localStorage.setItem('pcg.uiCache', JSON.stringify({ version: ui.version, loader: ui.loader, files: ui.files }));
          } catch (e) {}
          return ui;
        },
        function () {
          return cached() || bundled();
        },
      )
    : Promise.resolve(bundled());
  load
    .then(function (ui) {
      if (!ui) throw new Error('The panel files are missing: reinstall the Strata panel.');
      return start(ui);
    })
    .catch(function (e) {
      document.getElementById('boot').textContent = e.message;
    });
})();
