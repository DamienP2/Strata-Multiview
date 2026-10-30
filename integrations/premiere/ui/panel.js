/* Strata panel for Premiere Pro (CEP).
 *
 * Four screens, one gesture each:
 *  - Projects: the server's projects, kept in sync (auto-sync), search.
 *  - Templates: a project's templates (still picture from the server, zone,
 *    duration), search and zone filters; + (or a drag to the sequence)
 *    inserts one as it is at the playhead.
 *  - Clip: one template. An animated preview (the server's HTML5 renderer,
 *    /embed/preview) with play, loop and a playhead to scrub. Its timeline
 *    in the clip: total duration, the on-air point (IN / OUT, one marker as
 *    in the Designer), the pauses, the end. A new total moves every marker
 *    proportionally; each one can then be dragged anywhere. Its exposed
 *    parameters, then Insert at the playhead, or Update clip when a Strata
 *    clip of this template is selected in the sequence.
 *  - Settings (gear of the Projects screen): server, API key, sync, version.
 *
 * The server renders the clip at the active sequence's size and frame rate
 * (QuickTime, PNG with alpha, POST /api/render with the clip timing) and the
 * panel lays it on the first free video track above V1. A clip keeps its
 * values in a sidecar file (<clip>.pcg.json): selecting it in the timeline
 * opens it here, and Update clip replaces it in place.
 *
 * Written for CEP 9+ (Chromium 61): no optional chaining, no `??`.
 */
(function () {
  'use strict';

  var $ = function (id) {
    return document.getElementById(id);
  };
  var node = typeof require === 'function';
  var fs = node ? require('fs') : null;
  var path = node ? require('path') : null;
  var os = node ? require('os') : null;
  var crypto = node ? require('crypto') : null;

  var store = {
    get: function (k, d) {
      try {
        var v = localStorage.getItem('pcg.' + k);
        return v === null ? d : JSON.parse(v);
      } catch (e) {
        return d;
      }
    },
    set: function (k, v) {
      try {
        localStorage.setItem('pcg.' + k, JSON.stringify(v));
      } catch (e) {}
    },
  };

  /** Seconds on air of a template that holds until Take out. */
  var DEFAULT_HOLD = 5;
  /** Longest clip the server renders. */
  var MAX_SECONDS = 300;

  var S = {
    server: '',
    key: '',
    folder: store.get('folder', ''),
    online: null,
    screen: 'projects',
    projects: [],
    full: {},
    sync: store.get('syncState', {}),
    syncing: null,
    lastSync: 0,
    autoSync: store.get('autoSync', true),
    every: store.get('syncEvery', 30),
    q: '',
    tq: '',
    zone: 'all',
    project: null,
    template: null,
    model: null,
    origin: null,
    timing: null,
    data: {},
    seq: null,
    selection: null,
    selectedPath: '',
    editing: null,
    busy: {},
    update: null,
    test: null,
  };

  var esc = function (s) {
    return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c];
    });
  };
  var enc = encodeURIComponent;

  /** "4.0 s" in lists; "10 s" / "10.5 s" on the timeline. */
  function secs(x, fixed) {
    if (fixed) return x.toFixed(1) + ' s';
    return (Math.abs(x - Math.round(x)) < 0.005 ? String(Math.round(x)) : x.toFixed(1)) + ' s';
  }

  function ago(ts) {
    if (!ts) return '—';
    var s = Math.max(0, Math.round((Date.now() - ts) / 1000));
    if (s < 10) return 'just now';
    if (s < 60) return s + ' s ago';
    if (s < 3600) return Math.round(s / 60) + ' min ago';
    if (s < 86400) return Math.round(s / 3600) + ' h ago';
    return Math.round(s / 86400) + ' d ago';
  }

  function hash(text) {
    var h = 5381;
    for (var i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) | 0;
    return (h >>> 0).toString(36) + ':' + text.length;
  }

  var toastTimer = 0;
  function toast(msg, kind) {
    var el = $('toast');
    el.textContent = msg || '';
    el.className = 'toast ' + (msg ? 'on ' : '') + (kind || '');
    clearTimeout(toastTimer);
    if (msg) {
      toastTimer = setTimeout(function () {
        el.className = 'toast ' + (kind || '');
      }, kind === 'err' ? 6000 : 2500);
    }
  }

  // ------------------------------------------------------------------ host

  function evalScript(script) {
    return new Promise(function (resolve, reject) {
      if (!window.__adobe_cep__) return reject(new Error('Not running inside Premiere Pro'));
      window.__adobe_cep__.evalScript(script, function (res) {
        if (typeof res !== 'string' || res.indexOf('OK:') !== 0) reject(new Error(String(res).replace(/^ERR:/, '') || 'Premiere script error'));
        else resolve(res.slice(3));
      });
    });
  }
  var jsx = function (s) {
    return JSON.stringify(String(s));
  };

  /** Size, frame rate and playhead of the active sequence (seconds). */
  function readSequence() {
    return evalScript('pcg_sequenceInfo()').then(function (r) {
      var p = r.split('|');
      var fps = parseFloat(p[2]);
      return { width: +p[0], height: +p[1], fps: fps, rate: rateOf(fps), playhead: +p[3], name: p[4], tracks: +p[5] };
    });
  }

  function refreshSequence() {
    return readSequence().then(
      function (s) {
        S.seq = s;
        return s;
      },
      function (e) {
        S.seq = null;
        throw e;
      },
    );
  }

  function rateOf(fps) {
    var ntsc = [
      [24000, 1001],
      [30000, 1001],
      [60000, 1001],
    ];
    for (var i = 0; i < ntsc.length; i++) if (Math.abs(fps - ntsc[i][0] / ntsc[i][1]) < 0.01) return { num: ntsc[i][0], den: ntsc[i][1] };
    if (Math.abs(fps - Math.round(fps)) < 0.001) return { num: Math.round(fps), den: 1 };
    return { num: Math.round(fps * 1000), den: 1000 };
  }

  // ------------------------------------------------------------------- api

  function request(method, url, body) {
    return fetch(S.server + url, {
      method: method,
      cache: 'no-store',
      headers: Object.assign({ Authorization: 'Bearer ' + S.key }, body ? { 'Content-Type': 'application/json' } : {}),
      body: body ? JSON.stringify(body) : undefined,
    }).then(function (r) {
      if (!r.ok)
        return r.text().then(function (t) {
          throw new Error(t || r.status + ' ' + r.statusText);
        });
      return r;
    });
  }
  function api(method, url, body) {
    return request(method, url, body).then(function (r) {
      return r.json();
    });
  }

  function setOnline(ok) {
    S.online = ok;
    renderConn();
  }

  /** Inter, as in the app, from the server. */
  function loadFonts() {
    if (!S.server || $('pcgFonts')) return;
    var st = document.createElement('style');
    st.id = 'pcgFonts';
    st.textContent = [
      [400, 'Regular'],
      [500, 'Medium'],
      [600, 'SemiBold'],
      [700, 'Bold'],
      [800, 'ExtraBold'],
    ]
      .map(function (w) {
        return "@font-face{font-family:'Inter';font-weight:" + w[0] + ";src:url('" + S.server + '/fonts/Inter-' + w[1] + ".otf') format('opentype')}";
      })
      .join('\n');
    document.head.appendChild(st);
  }

  // ------------------------------------------------------------------ sync

  function syncAll() {
    if (!S.server || !S.key) {
      setOnline(null);
      return Promise.resolve();
    }
    if (S.syncing) return S.syncing;
    S.syncing = api('GET', '/api/projects')
      .then(function (list) {
        setOnline(true);
        S.projects = list;
        list.forEach(function (p) {
          if (!S.sync[p.id]) S.sync[p.id] = { sig: '', changed: 0 };
          if (!S.full[p.id]) S.sync[p.id].state = 'sync';
        });
        renderProjects();
        return list.reduce(function (chain, p) {
          return chain.then(function () {
            return syncProject(p.id);
          });
        }, Promise.resolve());
      })
      .catch(function () {
        setOnline(false);
      })
      .then(function () {
        S.syncing = null;
        S.lastSync = Date.now();
        var keep = {};
        S.projects.forEach(function (p) {
          var st = S.sync[p.id];
          if (st) keep[p.id] = { sig: st.sig, changed: st.changed };
        });
        store.set('syncState', keep);
        renderProjects();
        renderSettings();
      });
    return S.syncing;
  }

  function syncProject(id) {
    var st = S.sync[id];
    st.state = 'sync';
    renderProjects();
    return request('GET', '/api/projects/' + enc(id))
      .then(function (r) {
        return r.text();
      })
      .then(
        function (text) {
          var sig = hash(text);
          if (sig !== st.sig || !S.full[id]) {
            if (sig !== st.sig) st.changed = Date.now();
            st.sig = sig;
            S.full[id] = JSON.parse(text);
            if (S.project && S.project.id === id) projectChanged(S.full[id]);
          }
          st.state = 'ok';
        },
        function () {
          st.state = 'err';
        },
      )
      .then(renderProjects);
  }

  /** The open project changed on the server. */
  function projectChanged(p) {
    S.project = p;
    if (S.template) {
      var t = templateById(S.template.id);
      if (!t) {
        S.template = null;
        if (S.screen === 'clip') show('templates');
      } else if (JSON.stringify(t) !== JSON.stringify(S.template)) {
        var before = S.model;
        S.template = t;
        S.model = model(t);
        S.origin = originTiming(S.model);
        // Other stops (markers added or removed): back to the template's timing.
        if (!before || before.stops.length !== S.model.stops.length) S.timing = copyTiming(S.origin);
        t.fields.forEach(function (f) {
          if (!(f.key in S.data)) S.data[f.key] = f.default;
        });
        if (S.screen === 'clip') renderClip();
      }
    }
    if (S.screen === 'templates') renderTemplates();
  }

  // ------------------------------------------------------------- template timing

  function projectFps() {
    var r = S.project && S.project.frameRate;
    return r && r.num && r.den ? r.num / r.den : 25;
  }

  function templateById(id) {
    return (S.project ? S.project.templates : []).filter(function (t) {
      return t.id === id;
    })[0];
  }

  /**
   * The stops of a template, as the server counts them (render.rs `stops`):
   * its pauses and its on-air point, in frame order, one per frame. Each
   * with its hold in seconds (a timed pause, a timed on-air).
   */
  function model(t) {
    var fps = projectFps();
    var out = (t.markers || []).filter(function (m) {
      return m.kind === 'out';
    })[0];
    var hold = out ? Math.min(out.frame, t.duration) : t.duration;
    var after = function (m) {
      return m && m.resume && m.resume.type === 'after' ? Math.max(0, +m.resume.seconds || 0) : null;
    };
    var list = (t.markers || [])
      .filter(function (m) {
        return m.kind === 'pause';
      })
      .map(function (m) {
        return { frame: Math.max(0, Math.min(m.frame, t.duration)), onAir: false, hold: after(m) || 0 };
      });
    list.push({ frame: hold, onAir: true, hold: after(out) === null ? DEFAULT_HOLD : after(out) });
    list.sort(function (a, b) {
      return a.frame - b.frame || (b.onAir ? 1 : 0) - (a.onAir ? 1 : 0);
    });
    var stops = [];
    list.forEach(function (s) {
      var prev = stops[stops.length - 1];
      if (prev && Math.abs(prev.frame - s.frame) < 1e-6) {
        if (s.onAir) stops[stops.length - 1] = s;
        return;
      }
      stops.push(s);
    });
    var onAir = 0;
    stops.forEach(function (s, i) {
      if (s.onAir) onAir = i;
    });
    return { fps: fps, duration: t.duration, stops: stops, onAir: onAir };
  }

  /** The clip as the template plays on air: animations at their speed, the holds it sets. */
  function originTiming(m) {
    var t = 0;
    var prev = 0;
    var stops = m.stops.map(function (s) {
      t += (s.frame - prev) / m.fps + s.hold;
      prev = s.frame;
      return t;
    });
    var duration = t + (m.duration - prev) / m.fps;
    if (duration > MAX_SECONDS) {
      var r = MAX_SECONDS / duration;
      stops = stops.map(function (x) {
        return x * r;
      });
      duration = MAX_SECONDS;
    }
    return { stops: stops, duration: Math.max(duration, 0.04) };
  }

  function copyTiming(t) {
    return { stops: t.stops.slice(), duration: t.duration };
  }

  /** A clip made by an older panel: its seconds on air, with or without the out animation. */
  function timingFromHold(m, hold, withOut) {
    var t = 0;
    var prev = 0;
    var stops = m.stops.map(function (s, i) {
      t += (s.frame - prev) / m.fps + (i === m.onAir ? hold : 0);
      prev = s.frame;
      return t;
    });
    var duration = withOut === false ? stops[m.onAir] : t + (m.duration - prev) / m.fps;
    return { stops: stops, duration: Math.max(Math.min(duration, MAX_SECONDS), 0.04) };
  }

  function zoneName(id) {
    var z = ((S.project && S.project.zones) || []).filter(function (x) {
      return x.id === id;
    })[0];
    return z ? z.name : id || '';
  }

  // --------------------------------------------------------------- screens

  function show(screen) {
    S.screen = screen;
    ['projects', 'templates', 'clip', 'settings'].forEach(function (k) {
      $('scr-' + k).classList.toggle('hidden', k !== screen);
    });
    if (screen === 'projects') renderProjects();
    if (screen === 'templates') renderTemplates();
    if (screen === 'clip') renderClip();
    if (screen === 'settings') renderSettings(true);
  }

  // -------------------------------------------------------------- projects

  function renderConn() {
    var c = $('conn');
    var on = S.online;
    c.className = 'conn ' + (on === true ? 'ok' : on === false ? 'err' : '');
    $('connTxt').textContent = on === true ? 'Connected' : on === false ? 'Offline' : '';
    c.title = S.server;
    $('srvTxt').textContent = S.server ? 'Server ' + S.server.replace(/^https?:\/\//, '') : '';
    ['autoSync', 'autoSync2'].forEach(function (id) {
      $(id).classList.toggle('on', !!S.autoSync);
    });
    $('gear').classList.toggle('dot', S.update === 'install' || S.update === 'reload');
  }

  function renderProjects() {
    renderConn();
    if (S.screen !== 'projects') return;
    var q = S.q.trim().toLowerCase();
    var html = S.projects
      .filter(function (p) {
        return !q || p.name.toLowerCase().indexOf(q) >= 0;
      })
      .map(function (p) {
        var st = S.sync[p.id] || {};
        var full = S.full[p.id];
        var n = full ? full.templates.length : p.templates;
        var badge = st.state === 'sync' ? '<span class="badge busy">⟳ syncing…</span>' : st.state === 'err' ? '<span class="badge err">✕ error</span>' : '<span class="badge ok">✓ up to date</span>';
        return (
          '<button class="prj" data-project="' +
          esc(p.id) +
          '"><span class="av">' +
          esc((p.name.trim()[0] || '?').toUpperCase()) +
          '</span><span class="nm"><b>' +
          esc(p.name) +
          '</b><small>' +
          n +
          ' template' +
          (n === 1 ? '' : 's') +
          (st.changed ? ' · updated ' + ago(st.changed) : '') +
          '</small></span>' +
          badge +
          '<span class="chev">›</span></button>'
        );
      })
      .join('');
    // Redrawn every second: only when it changed (a click must not lose its row).
    if (html !== lastProjects) $('projects').innerHTML = lastProjects = html;
  }
  var lastProjects = null;

  function openProject(id) {
    var go = function (p) {
      S.project = p;
      S.zone = 'all';
      S.tq = '';
      $('tSearch').value = '';
      show('templates');
    };
    if (S.full[id]) return go(S.full[id]);
    api('GET', '/api/projects/' + enc(id)).then(
      function (p) {
        S.full[id] = p;
        go(p);
      },
      function (e) {
        toast(e.message, 'err');
      },
    );
  }

  // ------------------------------------------------------------- templates

  function thumbUrl(t, data) {
    return (
      S.server +
      '/api/projects/' +
      enc(S.project.id) +
      '/templates/' +
      enc(t.id) +
      '/thumbnail.png?key=' +
      enc(S.key) +
      (data ? '&data=' + enc(JSON.stringify(data)) : '') +
      '&v=' +
      (t.revision || 0) +
      '.' +
      JSON.stringify(t).length
    );
  }

  function renderTemplates() {
    if (!S.project) return;
    $('projTitle').textContent = S.project.name;
    var zones = [];
    S.project.templates.forEach(function (t) {
      if (t.zone && zones.indexOf(t.zone) < 0) zones.push(t.zone);
    });
    if (S.zone !== 'all' && zones.indexOf(S.zone) < 0) S.zone = 'all';
    $('zones').innerHTML = [['all', 'All']]
      .concat(
        zones.map(function (z) {
          return [z, zoneName(z)];
        }),
      )
      .map(function (c) {
        return '<button class="chip ' + (S.zone === c[0] ? 'on' : '') + '" data-zone="' + esc(c[0]) + '">' + esc(c[1]) + '</button>';
      })
      .join('');
    var q = S.tq.trim().toLowerCase();
    var box = $('templates');
    var rows = S.project.templates
      .filter(function (t) {
        if (S.zone !== 'all' && t.zone !== S.zone) return false;
        return !q || t.name.toLowerCase().indexOf(q) >= 0;
      })
      .map(templateRow);
    // The rows are kept (their pictures stay loaded): only their order changes.
    var same =
      rows.length === box.children.length &&
      rows.every(function (r, i) {
        return box.children[i] === r;
      });
    if (same) return;
    while (box.firstChild) box.removeChild(box.firstChild);
    rows.forEach(function (r) {
      box.appendChild(r);
    });
    renderBusy();
  }

  var rowCache = {};
  /** The row of a template, made again only when the template changed. */
  function templateRow(t) {
    var url = thumbUrl(t);
    var m = model(t);
    var html =
      '<span class="thumb"><img draggable="false" src="' +
      esc(url) +
      '"></span><span class="nm"><b>' +
      esc(t.name) +
      '</b><small>' +
      esc(zoneName(t.zone)) +
      ' · ' +
      secs(originTiming(m).duration, true) +
      '</small></span><button class="add" data-add="' +
      esc(t.id) +
      '" title="Insert at playhead">+</button>';
    var id = S.project.id + '/' + t.id;
    var c = rowCache[id];
    if (c && c.html === html) return c.el;
    var el = document.createElement('div');
    el.className = 'tpl';
    el.draggable = true;
    el.setAttribute('data-tpl', t.id);
    el.innerHTML = html;
    rowCache[id] = { html: html, el: el };
    return el;
  }

  // ------------------------------------------------------------------ clip

  /** Opens a template on the clip screen, with the values (and timing) of a clip when given. */
  function openTemplate(t, side) {
    S.template = t;
    S.model = model(t);
    S.origin = originTiming(S.model);
    S.data = {};
    t.fields.forEach(function (f) {
      S.data[f.key] = f.default;
    });
    if (side && side.data) Object.assign(S.data, side.data);
    if (side && side.timing && side.timing.stops && side.timing.stops.length === S.model.stops.length) S.timing = copyTiming(side.timing);
    else if (side && typeof side.hold === 'number') S.timing = timingFromHold(S.model, side.hold, side.withOut);
    else S.timing = copyTiming(S.origin);
    pv.t = onAirTime();
    pv.playing = false;
    $('play').innerHTML = '▶';
    $('play').classList.remove('on');
    show('clip');
  }

  function renderClip() {
    var t = S.template;
    if (!t) return;
    $('tplTitle').textContent = t.name;
    $('backTpl').textContent = '‹ ' + S.project.name;
    $('origin').textContent = '↺ Origin ' + secs(S.origin.duration);
    renderFields();
    loadPreview();
    sizePreview();
    renderTimeline();
    renderBusy();
  }

  function renderTotal() {
    var el = $('total');
    if (document.activeElement !== el) el.value = +S.timing.duration.toFixed(2);
  }

  /** Changes the clip's length: every marker keeps its place in proportion. */
  function setTotal(v) {
    v = Math.max(0.1, Math.min(MAX_SECONDS, v));
    var r = v / S.timing.duration;
    S.timing = {
      stops: S.timing.stops.map(function (x) {
        return x * r;
      }),
      duration: v,
    };
    renderTimeline();
  }

  var drag = null;

  /** Seconds shown across the timeline (frozen while a marker is dragged). */
  function viewSpan() {
    return drag ? drag.span : S.timing.duration * 1.06;
  }

  function renderTimeline() {
    if (!S.timing) return;
    renderTotal();
    var T = S.timing.duration;
    var V = viewSpan();
    var W = $('track').clientWidth || 240;
    var pc = function (x) {
      return (x / V) * 100 + '%';
    };
    var px = function (x) {
      return (x / V) * W;
    };
    var m = S.model;
    var onAir = S.timing.stops[m.onAir];

    // Ruler: a step of at least 44 px.
    var steps = [0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120];
    var step = steps.filter(function (s) {
      return px(s) >= 44;
    })[0] || 300;
    var endX = px(T);
    var ticks = [];
    for (var x = 0; x <= V + 1e-9; x += step) {
      if (px(x) > endX - 62 && x > 0) break;
      ticks.push('<span style="left:' + pc(x) + '">' + (x === 0 ? '0' : secs(x)) + '</span>');
    }
    ticks.push('<span class="endlab" style="left:' + pc(T) + ';transform:translateX(-100%)">END ' + secs(T) + '</span>');
    $('ruler').innerHTML = ticks.join('');

    // The bed: green up to the on-air point, red after it, to the end.
    var h = '<div class="bed"><span class="in" style="width:' + pc(onAir) + '"></span><span class="out" style="left:' + pc(onAir) + ';width:' + pc(T - onAir) + '"></span></div>';
    h += '<span class="mk end' + (drag && drag.i === 'end' ? ' drag' : '') + '" data-mk="end" style="left:' + pc(T) + '"></span>';
    S.timing.stops.forEach(function (s, i) {
      var kind = i === m.onAir ? 'onair' : 'pause';
      h += '<span class="mk ' + kind + (drag && drag.i === i ? ' drag' : '') + '" data-mk="' + i + '" style="left:' + pc(s) + '"></span>';
    });
    $('track').innerHTML = h;

    // Labels under the markers, on more rows when they would overlap.
    var rows = [];
    var labels = S.timing.stops
      .map(function (s, i) {
        var onair = i === m.onAir;
        var text = onair ? 'IN / OUT ' + secs(s) : 'PAUSE ' + secs(s);
        return { x: px(s), w: text.length * 5.8 + 6, onair: onair, html: onair ? '<em>IN</em> / <em>OUT</em> ' + secs(s) : 'PAUSE ' + secs(s) };
      })
      .sort(function (a, b) {
        return a.x - b.x;
      })
      .map(function (l) {
        var left = Math.max(0, Math.min(W - l.w, l.x - l.w / 2));
        var row = 0;
        while (row < rows.length && left < rows[row] + 4) row++;
        rows[row] = left + l.w;
        return '<span class="' + (l.onair ? 'onair' : 'pause') + '" style="left:' + left + 'px;top:' + row * 14 + 'px">' + l.html + '</span>';
      });
    $('labels').innerHTML = labels.join('');
    $('labels').style.height = Math.max(1, rows.length) * 14 + 'px';
    sendPreview();
  }

  /**
   * Drags a marker (a stop's index, or 'end') anywhere on the clip. The
   * stops keep their order: a marker pushes the ones it passes, and they go
   * back where they were when it comes back. The end follows the last stop.
   * Near the right edge the timeline zooms out, to make the clip longer.
   */
  function startDrag(e, which) {
    var rect = $('track').getBoundingClientRect();
    var i = which === 'end' ? 'end' : +which;
    var from = copyTiming(S.timing);
    var t0 = i === 'end' ? from.duration : from.stops[i];
    var snap = 1 / (S.seq ? S.seq.fps : projectFps());
    drag = { i: i, span: viewSpan(), x: e.clientX, cx: e.clientX };
    var place = function () {
      var t = t0 + ((drag.cx - drag.x) / rect.width) * drag.span;
      t = Math.max(0, Math.min(MAX_SECONDS, Math.round(t / snap) * snap));
      var st = from.stops.slice();
      var end = from.duration;
      if (i === 'end') {
        end = Math.max(t, snap);
        st = st.map(function (x) {
          return Math.min(x, end);
        });
      } else {
        st = st.map(function (x, k) {
          return k < i ? Math.min(x, t) : k > i ? Math.max(x, t) : t;
        });
        end = Math.max(end, t);
      }
      S.timing = { stops: st, duration: end };
      renderTimeline();
    };
    var edge = setInterval(function () {
      if (drag && drag.cx > rect.right - 6 && drag.span < MAX_SECONDS * 1.1) {
        drag.span *= 1.04;
        place();
      }
    }, 60);
    trackPointer(e, function (ev) {
      drag.cx = ev.clientX;
      place();
    }, function () {
      clearInterval(edge);
      drag = null;
      renderTimeline();
    });
    renderTimeline();
  }

  /**
   * Follows the mouse until it is released: mouse events, which every CEP
   * Chromium sends (pointer events are only used when the mouse ones are not).
   */
  function trackPointer(e, move, up) {
    e.preventDefault();
    var mouse = e.type === 'mousedown';
    var mv = mouse ? 'mousemove' : 'pointermove';
    var end = mouse ? 'mouseup' : 'pointerup';
    var onMove = function (ev) {
      ev.preventDefault();
      move(ev);
    };
    var onUp = function (ev) {
      window.removeEventListener(mv, onMove);
      window.removeEventListener(end, onUp);
      window.removeEventListener('blur', onUp);
      up(ev);
    };
    window.addEventListener(mv, onMove);
    window.addEventListener(end, onUp);
    window.addEventListener('blur', onUp);
  }

  /** Clicks and drags on the ruler or the bar move the playhead. */
  function startScrub(e) {
    var rect = $('track').getBoundingClientRect();
    var span = viewSpan();
    var at = function (ev) {
      setTime(((ev.clientX - rect.left) / rect.width) * span);
    };
    setPlaying(false);
    at(e);
    trackPointer(e, at, function () {});
  }

  // ---------------------------------------------------------------- preview

  /**
   * The template frame shown `t` seconds into the clip, as the server
   * renders it (render.rs `ClipTiming::frame_at`): an animation with time to
   * spare plays at its speed then holds on its stop, a shorter one plays
   * faster. Null once the graphic has gone out.
   */
  function clipFrame(t) {
    var m = S.model;
    var c = S.timing;
    var fromF = 0;
    var fromT = 0;
    for (var k = 0; k <= m.stops.length; k++) {
      var last = k === m.stops.length;
      var toF = last ? m.duration : m.stops[k].frame;
      var toT = last ? c.duration : c.stops[k];
      if (last || t < toT) {
        var anim = (toF - fromF) / m.fps;
        var len = toT - fromT;
        var d = Math.max(0, t - fromT);
        var f = len >= anim ? fromF + Math.min(d, anim) * m.fps : len > 0 ? fromF + Math.min(d / len, 1) * (toF - fromF) : toF;
        return !last || f < m.duration - 1e-6 ? f : null;
      }
      fromF = toF;
      fromT = toT;
    }
    return null;
  }

  /** When the graphic is fully on air (it reaches the IN / OUT point). */
  function onAirTime() {
    var m = S.model;
    var k = m.onAir;
    var fromT = k > 0 ? S.timing.stops[k - 1] : 0;
    var anim = (m.stops[k].frame - (k > 0 ? m.stops[k - 1].frame : 0)) / m.fps;
    return Math.min(S.timing.stops[k], fromT + anim);
  }

  var pv = { ready: false, page: '', loads: 0, playing: false, loop: store.get('loop', true), t: 0, last: 0 };

  function setTime(t) {
    pv.t = Math.max(0, Math.min(S.timing ? S.timing.duration : 0, t));
    sendPreview();
  }

  function setPlaying(on) {
    pv.playing = on;
    if (on && pv.t >= S.timing.duration - 1e-3) pv.t = 0;
    pv.last = 0;
    $('play').innerHTML = on ? '❚❚' : '▶';
    $('play').classList.toggle('on', on);
    if (on) requestAnimationFrame(tick);
  }

  function tick(now) {
    if (!pv.playing || S.screen !== 'clip' || !S.timing) return setPlaying(false);
    if (pv.last) {
      pv.t += (now - pv.last) / 1000;
      if (pv.t >= S.timing.duration) {
        if (pv.loop) pv.t = pv.t % S.timing.duration;
        else {
          pv.t = S.timing.duration;
          sendPreview();
          return setPlaying(false);
        }
      }
      sendPreview();
    }
    pv.last = now;
    requestAnimationFrame(tick);
  }

  /**
   * The preview page of the server (the HTML5 renderer). Loaded again for
   * another project: the page keeps the first project it read when two
   * arrive out of order.
   */
  function loadPreview() {
    var want = S.server + '|' + S.key + '|' + S.project.id;
    if (pv.page === want) return;
    pv.page = want;
    pv.ready = false;
    // A new address each time: never a page kept from before (it would not say it is ready).
    $('pvFrame').src = S.server + '/embed/preview?key=' + enc(S.key) + '&project=' + enc(S.project.id) + '&n=' + ++pv.loads;
  }

  window.addEventListener('message', function (ev) {
    if (ev.source !== $('pvFrame').contentWindow) return;
    if (ev.data && ev.data.type === 'pcg-ready') {
      pv.ready = true;
      sendPreview();
    }
  });

  /** The preview box at the sequence's (else the project's) shape. */
  function sizePreview() {
    var box = $('pv');
    var w = (S.seq && S.seq.width) || (S.project && S.project.width) || 16;
    var h = (S.seq && S.seq.height) || (S.project && S.project.height) || 9;
    box.style.height = Math.round(Math.min(box.clientWidth * (h / w), 260)) + 'px';
  }

  function sendPreview() {
    if (!S.template || !S.timing) return;
    var t = Math.min(pv.t, S.timing.duration);
    var x = (t / viewSpan()) * 100;
    $('ph').style.left = x + '%';
    $('pvTc').textContent = t.toFixed(2) + ' / ' + S.timing.duration.toFixed(2) + ' s';
    var f = clipFrame(t);
    var frame = $('pvFrame');
    frame.style.visibility = f === null ? 'hidden' : '';
    if (!pv.ready || f === null || !frame.contentWindow) return;
    frame.contentWindow.postMessage(
      {
        type: 'pcg-preview',
        projectId: S.project.id,
        templateId: S.template.id,
        data: S.data,
        frame: f,
        width: S.seq ? S.seq.width : undefined,
        height: S.seq ? S.seq.height : undefined,
      },
      '*',
    );
  }

  function renderFields() {
    var t = S.template;
    var box = $('fields');
    box.innerHTML = '';
    $('paramsCap').classList.toggle('hidden', !t.fields.length);
    t.fields.forEach(function (f) {
      var row = document.createElement('div');
      row.className = 'f' + (f.kind === 'multiline' || f.kind === 'table' ? ' top' : '');
      row.innerHTML = '<span title="' + esc(f.label) + '">' + esc(f.label) + '</span>';
      row.appendChild(fieldInput(f));
      box.appendChild(row);
    });
  }

  function segmented(choices, v, set) {
    var el = document.createElement('div');
    el.className = 'seg';
    el.innerHTML = choices
      .map(function (c) {
        return '<button type="button" class="' + (c[0] === v ? 'on' : '') + '" data-v="' + esc(c[0]) + '">' + esc(c[1]) + '</button>';
      })
      .join('');
    el.onclick = function (e) {
      var b = e.target.closest('button');
      if (!b) return;
      [].forEach.call(el.children, function (x) {
        x.classList.toggle('on', x === b);
      });
      set(b.getAttribute('data-v'));
    };
    return el;
  }

  function fieldInput(f) {
    var v = S.data[f.key] == null ? '' : String(S.data[f.key]);
    var set = function (x) {
      S.data[f.key] = x;
      sendPreview();
    };
    var el;
    if (f.options && f.options.length) {
      var short =
        f.options.length <= 4 &&
        f.options.every(function (o) {
          return String(o[1]).length <= 10;
        });
      if (short) return segmented(f.options, v, set);
      el = document.createElement('select');
      el.innerHTML = f.options
        .map(function (o) {
          return '<option value="' + esc(o[0]) + '">' + esc(o[1]) + '</option>';
        })
        .join('');
      el.value = v;
      el.onchange = function () {
        set(el.value);
      };
      return el;
    }
    if (f.kind === 'boolean') {
      var on = v === 'true' || v === '1';
      var num = v === '1' || v === '0';
      return segmented(
        [
          [num ? '1' : 'true', 'On'],
          [num ? '0' : 'false', 'Off'],
        ],
        on ? (num ? '1' : 'true') : num ? '0' : 'false',
        set,
      );
    }
    if (f.kind === 'color') {
      el = document.createElement('div');
      el.className = 'color';
      el.innerHTML = '<input type="color"><input spellcheck="false">';
      var pick = el.children[0];
      var hex = el.children[1];
      pick.value = /^#[0-9a-f]{6}/i.test(v) ? v.slice(0, 7) : '#ffffff';
      hex.value = v;
      pick.oninput = function () {
        hex.value = pick.value + (hex.value.length === 9 ? hex.value.slice(7) : '');
        set(hex.value);
      };
      hex.oninput = function () {
        if (/^#[0-9a-f]{6}/i.test(hex.value)) pick.value = hex.value.slice(0, 7);
        set(hex.value);
      };
      return el;
    }
    el = document.createElement(f.kind === 'multiline' || f.kind === 'table' ? 'textarea' : 'input');
    if (f.kind === 'multiline' || f.kind === 'table') el.rows = 2;
    if (f.kind === 'number') {
      el.type = 'number';
      if (f.min != null) el.min = f.min;
      if (f.max != null) el.max = f.max;
    }
    el.spellcheck = false;
    el.value = v;
    el.oninput = function () {
      set(el.value);
    };
    return el;
  }

  // ---------------------------------------------------------------- render

  function mkdirs(dir) {
    if (fs.existsSync(dir)) return;
    mkdirs(path.dirname(dir));
    fs.mkdirSync(dir);
  }

  function outputFolder(project) {
    var base = S.folder ? S.folder : path.join(os.homedir(), 'Documents', 'Strata Renders');
    var dir = path.join(base, project.name.replace(/[\\/:*?"<>|]/g, '-'));
    mkdirs(dir);
    return dir;
  }

  function labelOf(t, data) {
    var first = t.fields
      .filter(function (f) {
        return f.kind === 'text' || f.kind === 'multiline';
      })
      .map(function (f) {
        return data[f.key];
      })
      .filter(Boolean)[0];
    return String(first || t.name).replace(/\n/g, ' ');
  }

  /** What the server renders, and the file it goes to. */
  function job(project, t, data, timing) {
    var m = model(t);
    var s = S.seq;
    var onAir = timing.stops[m.onAir];
    var body = {
      projectId: project.id,
      templateId: t.id,
      data: Object.assign({}, data),
      width: s.width,
      height: s.height,
      frameRate: s.rate,
      timing: { stops: timing.stops.slice(), duration: timing.duration },
      // For older readers of the sidecar: seconds on air, out animation or not.
      hold: Math.max(0, onAir - m.stops[m.onAir].frame / m.fps),
      includeOut: timing.duration > onAir + 1e-6,
    };
    var digest = crypto
      .createHash('md5')
      .update(JSON.stringify([body, t.revision || 0, JSON.stringify(t).length]))
      .digest('hex')
      .slice(0, 10);
    var label = labelOf(t, data);
    var file = path.join(outputFolder(project), (t.name + ' - ' + label).replace(/[\\/:*?"<>|\n]/g, '-').slice(0, 70) + ' ' + digest + '.mov');
    return { body: body, file: file, label: label, template: t };
  }

  var chain = Promise.resolve();
  /** Renders `j` (one render at a time; the same graphic rendered before is reused), then `place` lays it. */
  function enqueue(j, place, busyKey) {
    S.busy[busyKey] = (S.busy[busyKey] || 0) + 1;
    renderBusy();
    var run = function () {
      var made = fs.existsSync(j.file)
        ? Promise.resolve()
        : request('POST', '/api/render', j.body)
            .then(function (r) {
              return r.arrayBuffer();
            })
            .then(function (buf) {
              fs.writeFileSync(j.file, Buffer.from(buf));
            });
      return made
        .then(function () {
          var b = j.body;
          fs.writeFileSync(j.file + '.pcg.json', JSON.stringify({ projectId: b.projectId, templateId: b.templateId, data: b.data, timing: b.timing, hold: b.hold, withOut: b.includeOut, label: j.label }, null, 2));
          return place(j.file);
        })
        .then(function (msg) {
          toast('✓ ' + msg, 'ok');
        })
        .catch(function (e) {
          toast(e.message, 'err');
        })
        .then(function () {
          S.busy[busyKey]--;
          renderBusy();
        });
    };
    chain = chain.then(run);
    return chain;
  }

  /** A Strata clip of this template selected in the sequence: Update clip, else Insert. */
  function renderBusy() {
    var editing = !!(S.editing && S.template && S.editing.templateId === S.template.id && S.editing.projectId === S.project.id);
    var u = $('update');
    u.classList.toggle('hidden', !editing);
    u.classList.toggle('busy', !!S.busy.update);
    $('insert').className = 'btn ' + (editing ? 'ghost' : 'primary') + (S.busy.insert ? ' busy' : '');
    [].forEach.call(document.querySelectorAll('[data-add]'), function (b) {
      b.classList.toggle('busy', !!S.busy['add:' + b.getAttribute('data-add')]);
    });
  }

  function needNode() {
    if (node) return true;
    toast('Node.js is disabled in this panel.', 'err');
    return false;
  }

  function trackFor(start, len) {
    return evalScript('pcg_freeTrack(' + start + ', ' + (start + len) + ')').then(Number);
  }

  /** Lays `j` on the first free track at the playhead. */
  function insertJob(makeJob, busyKey) {
    if (!needNode()) return;
    refreshSequence()
      .then(function (s) {
        var j = makeJob();
        var at = s.playhead;
        return trackFor(at, j.body.timing.duration).then(function (track) {
          enqueue(
            j,
            function (file) {
              return evalScript('pcg_import(' + jsx(file) + ', ' + track + ', ' + at + ')');
            },
            busyKey,
          );
        });
      })
      .catch(function (e) {
        toast(e.message, 'err');
      });
  }

  function insert() {
    if (!S.template) return;
    var p = S.project;
    var t = S.template;
    var data = Object.assign({}, S.data);
    var timing = copyTiming(S.timing);
    insertJob(function () {
      return job(p, t, data, timing);
    }, 'insert');
  }

  /** A template as it is (its defaults, its timing), from the list. */
  function defaultJob(t) {
    var data = {};
    t.fields.forEach(function (f) {
      data[f.key] = f.default;
    });
    return job(S.project, t, data, originTiming(model(t)));
  }

  function addTemplate(t) {
    insertJob(function () {
      return defaultJob(t);
    }, 'add:' + t.id);
  }

  /** Renders the clip again and puts it in place of the selected Strata clip. */
  function update() {
    if (!S.template || !S.editing || !needNode()) return;
    var p = S.project;
    var t = S.template;
    var data = Object.assign({}, S.data);
    var timing = copyTiming(S.timing);
    refreshSequence()
      .then(function () {
        enqueue(
          job(p, t, data, timing),
          function (file) {
            return evalScript('pcg_replaceSelected(' + jsx(file) + ')');
          },
          'update',
        );
      })
      .catch(function (e) {
        toast(e.message, 'err');
      });
  }

  // ------------------------------------------------------------- selection

  /** The selected clip; a Strata clip opens here. */
  function watchSelection() {
    if (!window.__adobe_cep__ || S.screen === 'settings') return;
    evalScript('pcg_selected()')
      .then(function (r) {
        if (!r) {
          S.selection = null;
          S.selectedPath = '';
          if (S.editing) {
            S.editing = null;
            renderBusy();
          }
          return;
        }
        var p = r.split('\t');
        S.selection = { media: p[0], start: +p[1], end: +p[2], track: +p[3] };
        if (p[0] === S.selectedPath) return;
        S.selectedPath = p[0];
        var side = null;
        try {
          side = node && p[0] && fs.existsSync(p[0] + '.pcg.json') ? JSON.parse(fs.readFileSync(p[0] + '.pcg.json', 'utf8')) : null;
        } catch (e) {}
        S.editing = side ? { media: p[0], projectId: side.projectId, templateId: side.templateId } : null;
        renderBusy();
        if (!side) return;
        var open = function (project) {
          S.project = project;
          var t = templateById(side.templateId);
          if (t) openTemplate(t, side);
        };
        if (S.full[side.projectId]) open(S.full[side.projectId]);
        else
          api('GET', '/api/projects/' + enc(side.projectId)).then(function (pr) {
            S.full[pr.id] = pr;
            open(pr);
          });
      })
      .catch(function () {});
  }

  // -------------------------------------------------------------- settings

  function loadSettings() {
    S.server = (store.get('server', '') || '').replace(/\/+$/, '');
    S.key = store.get('key', '');
    $('server').value = S.server;
    $('key').value = S.key;
    $('every').value = String(S.every);
  }

  /**
   * Keeps what was typed: true when it changed, null when the panel reloads
   * (a new server serves its own panel, through the loader).
   */
  function saveSettings() {
    var server = $('server').value.trim().replace(/\/+$/, '');
    if (server && !/^https?:\/\//.test(server)) server = 'http://' + server;
    var key = $('key').value.trim();
    var changed = server !== S.server || key !== S.key;
    S.server = server;
    S.key = key;
    store.set('server', server);
    store.set('key', key);
    var L = window.STRATA_LOADER;
    if (L && server && server !== L.server) {
      location.reload();
      return null;
    }
    if (changed) {
      S.full = {};
      S.projects = [];
      loadFonts();
    }
    return changed;
  }

  function testConnection() {
    if (saveSettings() === null) return;
    if (!S.server || !S.key) {
      S.test = { kind: 'err', text: 'Not set' };
      return renderSettings();
    }
    S.test = { kind: 'busy', text: '…' };
    renderSettings();
    var t0 = Date.now();
    api('GET', '/api/projects').then(
      function () {
        S.test = { kind: 'ok', text: 'Connected · ' + (Date.now() - t0) + ' ms' };
        setOnline(true);
        renderSettings();
        syncAll();
      },
      function (e) {
        var msg = String(e.message || '').trim();
        S.test = { kind: 'err', text: msg.length > 40 || /fetch/i.test(msg) ? 'Unreachable' : msg };
        setOnline(false);
        renderSettings();
      },
    );
  }

  function renderSettings(fresh) {
    renderConn();
    if (S.screen !== 'settings') return;
    if (fresh) {
      $('server').value = S.server;
      $('key').value = S.key;
    }
    var st = S.test;
    $('testState').className = 'state ell ' + (st ? st.kind : '');
    $('testState').innerHTML = st ? '<i></i>' + esc(st.text) : '';
    $('every').value = String(S.every);
    $('lastAgo').textContent = S.syncing ? '⟳' : ago(S.lastSync);
    var L = window.STRATA_LOADER;
    $('ver').textContent = L ? L.ui : 'dev';
    var u = S.update;
    var vs = $('verState');
    vs.className = 'state ' + (u === 'ok' ? 'ok' : u === 'install' || u === 'reload' ? 'busy' : u === 'err' ? 'err' : '');
    vs.textContent = u === 'ok' ? '✓ up to date' : u === 'install' || u === 'reload' ? '● new version' : u === 'err' ? '✕' : u === 'checking' ? '…' : '';
    $('check').textContent = u === 'install' ? 'Update' : u === 'reload' ? 'Reload' : 'Check for updates';
    $('check').disabled = u === 'checking' || u === 'installing';
  }

  /** The server's panel against this one: a newer loader to install, or a newer panel to load. */
  function checkUpdate() {
    var L = window.STRATA_LOADER;
    if (!S.server) return;
    S.update = 'checking';
    renderSettings();
    fetch(S.server + '/premiere/manifest.json', { cache: 'no-store' })
      .then(function (r) {
        if (!r.ok) throw new Error(r.status);
        return r.json();
      })
      .then(
        function (m) {
          if (L && (m.loader || 0) > L.version) S.update = 'install';
          else if (L && m.version !== L.ui) S.update = 'reload';
          else S.update = 'ok';
          renderSettings();
        },
        function () {
          S.update = 'err';
          renderSettings();
        },
      );
  }

  function updateNow() {
    var L = window.STRATA_LOADER;
    if (S.update === 'reload' || !L) return location.reload();
    S.update = 'installing';
    renderSettings();
    L.install().then(
      function (r) {
        if (r.restart) {
          S.update = 'ok';
          renderSettings();
          toast('Restart Premiere Pro', 'ok');
        } else r.reload();
      },
      function (e) {
        S.update = 'err';
        renderSettings();
        toast(e.message, 'err');
      },
    );
  }

  // ------------------------------------------------------------------ wire

  document.addEventListener('click', function (e) {
    var el = e.target.closest('button, [data-tpl]');
    if (!el) return;
    var d = el.dataset;
    if (d.go) {
      if (d.go === 'projects') renderProjects();
      return show(d.go);
    }
    if (d.project) return openProject(d.project);
    if (d.zone) {
      S.zone = d.zone;
      return renderTemplates();
    }
    if (d.add) {
      e.stopPropagation();
      var ta = templateById(d.add);
      if (ta) addTemplate(ta);
      return;
    }
    if (d.tpl) {
      var t = templateById(d.tpl);
      if (t) openTemplate(t);
    }
  });

  // Markers drag, the ruler and the bar scrub (mouse events: every CEP Chromium).
  $('tl').addEventListener('mousedown', function (e) {
    if (e.button !== 0) return;
    var mk = e.target.closest('[data-mk]');
    if (mk) startDrag(e, mk.getAttribute('data-mk'));
    else if (e.target.closest('#ruler, #track')) startScrub(e);
  });
  $('play').onclick = function () {
    setPlaying(!pv.playing);
  };
  $('loop').classList.toggle('on', pv.loop);
  $('loop').onclick = function () {
    pv.loop = !pv.loop;
    store.set('loop', pv.loop);
    this.classList.toggle('on', pv.loop);
  };

  $('pSearch').oninput = function () {
    S.q = this.value;
    renderProjects();
  };
  $('tSearch').oninput = function () {
    S.tq = this.value;
    renderTemplates();
  };
  $('total').oninput = function () {
    var v = parseFloat(this.value);
    if (v > 0) setTotal(v);
  };
  $('total').onchange = function () {
    renderTotal();
  };
  $('total').onblur = function () {
    this.value = +S.timing.duration.toFixed(2);
  };
  $('origin').onclick = function () {
    S.timing = copyTiming(S.origin);
    renderTimeline();
  };
  $('update').onclick = update;
  $('insert').onclick = insert;
  $('gear').onclick = function () {
    show('settings');
  };
  $('syncNow').onclick = $('syncNow2').onclick = function () {
    syncAll();
    renderSettings();
  };
  $('autoSync').onclick = $('autoSync2').onclick = function () {
    S.autoSync = !S.autoSync;
    store.set('autoSync', S.autoSync);
    renderConn();
  };
  $('every').onchange = function () {
    S.every = +this.value || 30;
    store.set('syncEvery', S.every);
  };
  $('server').onchange = $('key').onchange = function () {
    if (saveSettings()) syncAll();
  };
  $('test').onclick = testConnection;
  $('paste').onclick = function () {
    var k = $('key');
    var done = function (text) {
      k.value = String(text || '').trim();
      if (saveSettings()) syncAll();
    };
    if (navigator.clipboard && navigator.clipboard.readText) navigator.clipboard.readText().then(done, function () {});
    else {
      k.focus();
      k.select();
      if (document.execCommand('paste')) done(k.value);
    }
  };
  $('check').onclick = function () {
    if (S.update === 'install' || S.update === 'reload') updateNow();
    else checkUpdate();
  };

  // A template dragged to the sequence: its last render goes there as a
  // file; never rendered yet, it is inserted at the playhead.
  var dragged = null;
  document.addEventListener('dragstart', function (e) {
    var row = e.target.closest && e.target.closest('[data-tpl]');
    if (!row) return;
    var t = templateById(row.getAttribute('data-tpl'));
    if (!t) return;
    dragged = { t: t, file: null, left: false, dropped: false };
    e.dataTransfer.effectAllowed = 'copy';
    e.dataTransfer.setData('text/plain', t.name);
    if (node && S.seq) {
      try {
        var f = defaultJob(t).file;
        if (fs.existsSync(f)) {
          dragged.file = f;
          e.dataTransfer.setData('com.adobe.cep.dnd.file.0', f);
        }
      } catch (err) {}
    }
  });
  document.addEventListener('dragleave', function (e) {
    if (dragged) dragged.left = e.clientX <= 0 || e.clientY <= 0 || e.clientX >= window.innerWidth || e.clientY >= window.innerHeight;
  });
  document.addEventListener('dragover', function () {
    if (dragged) dragged.left = false;
  });
  document.addEventListener('drop', function () {
    if (dragged) dragged.dropped = true;
  });
  document.addEventListener('dragend', function () {
    var d = dragged;
    dragged = null;
    if (d && !d.file && !d.dropped && d.left) addTemplate(d.t);
  });

  window.addEventListener('resize', function () {
    if (S.screen === 'clip') {
      sizePreview();
      renderTimeline();
    }
  });
  // Back in the panel: the sequence may have changed.
  window.addEventListener('focus', function () {
    refreshSequence().catch(function () {});
  });

  // Every second: the selected clip, the "… ago", auto-sync when due.
  setInterval(function () {
    watchSelection();
    if (S.autoSync && !S.syncing && S.server && S.key && Date.now() - S.lastSync >= S.every * 1000) syncAll();
    if (S.screen === 'projects') renderProjects();
    if (S.screen === 'settings') renderSettings();
  }, 1000);

  loadSettings();
  loadFonts();
  var L0 = window.STRATA_LOADER;
  if (L0 && L0.updateAvailable) S.update = 'install';
  renderConn();
  if (!S.server || !S.key) show('settings');
  else {
    show('projects');
    syncAll();
  }
  refreshSequence().catch(function () {});
})();
