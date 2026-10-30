/* Strata panel — Premiere Pro side (ExtendScript).
   Returns plain "OK:…" / "ERR:…" strings: old ExtendScript has no JSON.
   Lists come back one item per line, fields separated by tabs. */

function pcg_clean(s) {
  return String(s == null ? "" : s).replace(/[\t\r\n]+/g, " ");
}

/** "width|height|fps|playhead|name|videoTracks|in|out" of the active sequence (seconds). */
function pcg_sequenceInfo() {
  var s = app.project.activeSequence;
  if (!s) return "ERR:Open a sequence first.";
  var ticksPerFrame = Number(s.timebase);
  var fps = ticksPerFrame > 0 ? 254016000000 / ticksPerFrame : 25;
  var pos = s.getPlayerPosition();
  var inPt = -1;
  var outPt = -1;
  try {
    inPt = s.getInPointAsTime().seconds;
    outPt = s.getOutPointAsTime().seconds;
  } catch (e) {}
  return "OK:" + s.frameSizeHorizontal + "|" + s.frameSizeVertical + "|" + fps + "|" + pos.seconds + "|" + pcg_clean(s.name).replace(/\|/g, "/") + "|" + s.videoTracks.numTracks + "|" + inPt + "|" + outPt;
}

function pcg_bin(name) {
  var root = app.project.rootItem;
  for (var i = 0; i < root.children.numItems; i++) {
    var c = root.children[i];
    if (c.type === ProjectItemType.BIN && c.name === name) return c;
  }
  return root.createBin(name);
}

function pcg_norm(p) {
  return String(p).replace(/\\/g, "/").toLowerCase();
}

/** The first selected clip of a video track: [trackItem, trackIndex]. */
function pcg_selection() {
  var s = app.project.activeSequence;
  if (!s) return null;
  for (var t = 0; t < s.videoTracks.numTracks; t++) {
    var tr = s.videoTracks[t];
    for (var i = 0; i < tr.clips.numItems; i++) {
      var c = tr.clips[i];
      if (c.isSelected()) return [c, t];
    }
  }
  return null;
}

/** "mediaPath\tstart\tend\ttrack" of the selected clip, or "OK:" when nothing is selected. */
function pcg_selected() {
  var sel = pcg_selection();
  if (!sel) return "OK:";
  var c = sel[0];
  var media = "";
  try {
    media = c.projectItem ? c.projectItem.getMediaPath() : "";
  } catch (e) {}
  return "OK:" + pcg_clean(media) + "\t" + c.start.seconds + "\t" + c.end.seconds + "\t" + sel[1];
}

/** The first video track above V1 with nothing between start and end (seconds); the top one when none is free. */
function pcg_freeTrack(start, end) {
  var s = app.project.activeSequence;
  if (!s) return "ERR:Open a sequence first.";
  var n = s.videoTracks.numTracks;
  for (var t = 1; t < n; t++) {
    var tr = s.videoTracks[t];
    var busy = false;
    for (var i = 0; i < tr.clips.numItems; i++) {
      var c = tr.clips[i];
      if (c.start.seconds < end - 0.001 && c.end.seconds > start + 0.001) {
        busy = true;
        break;
      }
    }
    if (!busy) return "OK:" + t;
  }
  return "OK:" + (n - 1);
}

function pcg_importItem(path) {
  var bin = pcg_bin("Strata");
  if (!app.project.importFiles([path], true, bin, false)) return null;
  for (var i = bin.children.numItems - 1; i >= 0; i--) {
    var c = bin.children[i];
    if (c.getMediaPath && pcg_norm(c.getMediaPath()) === pcg_norm(path)) return c;
  }
  return null;
}

/** Imports `path` into the "Strata" bin; with a track index, also lays it
    on that video track at `at` seconds (the playhead when negative). */
function pcg_import(path, trackIndex, at) {
  var s = app.project.activeSequence;
  var item = pcg_importItem(path);
  if (!item) return "ERR:Premiere could not import " + path;
  if (trackIndex === "" || trackIndex === null || trackIndex < 0) return "OK:" + item.name + " imported";
  if (!s) return "ERR:Open a sequence to place the clip.";
  var t = Math.min(Number(trackIndex), s.videoTracks.numTracks - 1);
  var when = at === undefined || at < 0 ? s.getPlayerPosition().seconds : Number(at);
  s.videoTracks[t].overwriteClip(item, when);
  return "OK:" + item.name + " placed on V" + (t + 1);
}

/** Replaces the selected clip by `path`, at the same place and track. */
function pcg_replaceSelected(path) {
  var s = app.project.activeSequence;
  var sel = pcg_selection();
  if (!s || !sel) return "ERR:Select the Strata clip to update in the timeline.";
  var start = sel[0].start.seconds;
  var t = sel[1];
  var item = pcg_importItem(path);
  if (!item) return "ERR:Premiere could not import " + path;
  try {
    sel[0].remove(false, false);
  } catch (e) {}
  s.videoTracks[t].overwriteClip(item, start);
  return "OK:" + item.name + " updated on V" + (t + 1);
}
