(() => {
  const DEFAULTS = { tx: 0, ty: 0, rot: 0, sx: 1, sy: 1, op: 1, vis: 1 };
  const FONT = '"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';
  const SVG = "http://www.w3.org/2000/svg";

  const clamp01 = (value) => Math.max(0, Math.min(1, value));

  function bezier(x1, y1, x2, y2, x) {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    let low = 0;
    let high = 1;
    let t = x;
    for (let i = 0; i < 40; i += 1) {
      t = (low + high) / 2;
      const bx = 3 * (1 - t) * (1 - t) * t * x1 + 3 * (1 - t) * t * t * x2 + t * t * t;
      if (bx < x) low = t;
      else high = t;
    }
    return 3 * (1 - t) * (1 - t) * t * y1 + 3 * (1 - t) * t * t * y2 + t * t * t;
  }

  function easeBase(easing) {
    if (easing.t === "exp") {
      const k = easing.k ?? 2;
      return (u) => (k === 0 ? u : (Math.exp(k * u) - 1) / (Math.exp(k) - 1));
    }
    if (easing.t === "back") {
      const a = easing.a ?? 1;
      return (u) => u * u * u - u * a * Math.sin(u * Math.PI);
    }
    return (u) => u;
  }

  function ease(easing, u) {
    const base = easeBase(easing);
    if (easing.m === "EaseOut") return 1 - base(1 - u);
    if (easing.m === "EaseInOut") return u < 0.5 ? base(2 * u) / 2 : 1 - base(2 - 2 * u) / 2;
    return base(u);
  }

  function progress(frame, u) {
    if (frame.k === "d") return u >= 1 ? 1 : 0;
    if (frame.k === "s" && frame.s)
      return bezier(frame.s[0], frame.s[1], frame.s[2], frame.s[3], u);
    if (frame.k === "e" && frame.e) return ease(frame.e, u);
    return u;
  }

  function keyframeValue(frames, t, from) {
    let previousTime = 0;
    let previousValue = from;
    for (const frame of frames) {
      if (t < frame.t) {
        const span = frame.t - previousTime;
        const u = span > 0 ? (t - previousTime) / span : 1;
        return previousValue + (frame.v - previousValue) * progress(frame, u);
      }
      previousTime = frame.t;
      previousValue = frame.v;
    }
    return previousValue;
  }

  function fromToValue(track, t, snapshot) {
    const from = track.from ?? snapshot;
    const period = track.rev ? track.dur * 2 : track.dur;
    const repeats = track.rep === -1 ? Number.POSITIVE_INFINITY : track.rep || 1;
    let u;
    if (t >= period * repeats) u = track.rev ? 0 : 1;
    else {
      const local = t % period;
      u = local <= track.dur ? local / track.dur : 2 - local / track.dur;
    }
    const eased = track.e ? ease(track.e, u) : u;
    return from + (track.to - from) * eased;
  }

  function trackEnd(track) {
    const begin = track.begin || 0;
    if (track.kf) return begin + track.kf[track.kf.length - 1].t;
    if (track.rep === -1) return Number.POSITIVE_INFINITY;
    return begin + track.dur * (track.rev ? 2 : 1) * (track.rep || 1);
  }

  function boardLength(board) {
    if (board.dur != null) return board.dur;
    return board.tracks.reduce((end, track) => Math.max(end, trackEnd(track)), 0);
  }

  function hideTime(spec, panel) {
    const exit = spec[panel.panel].states.ExitMessages;
    return panel.seq.hide + (exit ? boardLength(exit) : 0);
  }

  function scaleTrack(track, factor) {
    const copy = { ...track };
    if (copy.kf) copy.kf = copy.kf.map((frame) => ({ ...frame, v: frame.v * factor }));
    if (copy.to != null) copy.to *= factor;
    if (copy.from != null) copy.from *= factor;
    return copy;
  }

  function namedElements(node, names = []) {
    if (node.name) names.push(node.name);
    for (const child of node.children || []) namedElements(child, names);
    return names;
  }

  function measureWidth(widths, toast) {
    const text = Math.max(...widths);
    return Math.min(toast.maxWidth, Math.max(toast.minWidth, Math.ceil(text + toast.textPad)));
  }

  function hexToRgb(hex) {
    const value = Number.parseInt(String(hex).replace("#", ""), 16);
    if (Number.isNaN(value)) return [16, 124, 16];
    return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
  }

  function rgbToHsl([r, g, b]) {
    const red = r / 255;
    const green = g / 255;
    const blue = b / 255;
    const max = Math.max(red, green, blue);
    const min = Math.min(red, green, blue);
    const light = (max + min) / 2;
    if (max === min) return [0, 0, light];
    const delta = max - min;
    const saturation = light > 0.5 ? delta / (2 - max - min) : delta / (max + min);
    let hue;
    if (max === red) hue = (green - blue) / delta + (green < blue ? 6 : 0);
    else if (max === green) hue = (blue - red) / delta + 2;
    else hue = (red - green) / delta + 4;
    return [hue / 6, saturation, light];
  }

  function hslToRgb([hue, saturation, light]) {
    if (saturation === 0) return [light, light, light].map((c) => Math.round(c * 255));
    const q = light < 0.5 ? light * (1 + saturation) : light + saturation - light * saturation;
    const p = 2 * light - q;
    const channel = (offset) => {
      let t = hue + offset;
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1 / 6) return p + (q - p) * 6 * t;
      if (t < 1 / 2) return q;
      if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
      return p;
    };
    return [channel(1 / 3), channel(0), channel(-1 / 3)].map((c) => Math.round(c * 255));
  }

  function accentShades(hex, shades) {
    const rgb = hexToRgb(hex);
    const [hue, saturation, light] = rgbToHsl(rgb);
    const css = ([r, g, b]) => `rgb(${r}, ${g}, ${b})`;
    return {
      accent: css(rgb),
      light: css(hslToRgb([hue, saturation, clamp01(light + shades.light)])),
      dark: css(hslToRgb([hue, saturation, clamp01(light + shades.dark)])),
    };
  }

  function createTimeline(spec, layout, kind, width) {
    const panel = layout.panels[kind];
    const states = spec[panel.panel].states;
    const boards = spec[panel.panel].boards || {};
    const names = namedElements(panel.toast);
    const scaled = layout.scaled;
    const prepare = (board) => ({
      ...board,
      tracks: board.tracks.map((track) => {
        const rule = scaled[`${track.el}.${track.prop}`];
        return rule ? scaleTrack(track, (width / 2 + rule.offset) / rule.reference) : track;
      }),
    });
    const schedule = [{ at: 0, board: prepare(states.Show) }];
    for (const [name, board] of Object.entries(boards)) {
      schedule.push({ at: panel.seq[name] ?? 0, board: prepare(board) });
    }
    if (panel.seq.ambient != null) {
      schedule.push({
        at: panel.seq.ambient,
        board: {
          tracks: [
            { el: "IconIntroSequence", prop: "op", kf: [{ t: 0, v: 0, k: "d" }] },
            { el: "IconAmbientLoopSequence", prop: "op", kf: [{ t: 0, v: 1, k: "d" }] },
          ],
        },
      });
    }
    schedule.push({ at: panel.seq.rotate, board: prepare(states.RotateMessages) });
    if (states.ExitMessages) {
      schedule.push({ at: panel.seq.hide, board: prepare(states.ExitMessages) });
    }
    schedule.push({
      at: hideTime(spec, panel),
      board: prepare(states.Hide),
      replaces: schedule[0],
    });
    for (const entry of schedule) entry.length = boardLength(entry.board);

    const bases = {};
    for (const name of names) bases[name] = { ...DEFAULTS, ...(panel.bases[name] || {}) };
    if (bases.AuroraFlipBook) {
      bases.AuroraFlipBook.sx = (width + layout.auroraScale.x[1]) / layout.auroraScale.x[0];
      bases.AuroraFlipBook.sy = layout.auroraScale.y;
    }
    let current = structuredCopy(bases);
    const active = [];

    function structuredCopy(source) {
      const copy = {};
      for (const name of Object.keys(source)) copy[name] = { ...source[name] };
      return copy;
    }

    function step(t) {
      for (const entry of schedule) {
        if (entry.started || t < entry.at) continue;
        entry.started = true;
        entry.snapshot = {};
        for (const track of entry.board.tracks) {
          const value = current[track.el]?.[track.prop];
          entry.snapshot[`${track.el}.${track.prop}`] = value ?? DEFAULTS[track.prop] ?? 0;
        }
        if (entry.replaces) {
          const index = active.indexOf(entry.replaces);
          if (index >= 0) active.splice(index, 1);
        }
        active.push(entry);
      }
      const next = structuredCopy(bases);
      for (const entry of active) {
        const board = entry.board;
        let local = (t - entry.at - (board.begin || 0)) * (board.speed || 1);
        if (local < 0) continue;
        if (board.rep > 1 && entry.length > 0 && Number.isFinite(entry.length)) {
          const total = entry.length * board.rep;
          local = local >= total ? entry.length : local % entry.length;
        }
        for (const track of board.tracks) {
          const target = next[track.el];
          if (!target) continue;
          const trackTime = local - (track.begin || 0);
          if (trackTime < 0) continue;
          const snapshot = entry.snapshot[`${track.el}.${track.prop}`];
          target[track.prop] = track.kf
            ? keyframeValue(track.kf, trackTime, snapshot)
            : fromToValue(track, trackTime, snapshot);
        }
      }
      current = next;
      return current;
    }

    return { step, names };
  }

  function frameIndex(flipbook, elapsed) {
    const count = Number(flipbook.FrameCount);
    const fps = Number(flipbook.DesiredFrameRate);
    let frame = Math.max(0, Math.floor(elapsed * fps + 1e-6));
    if (flipbook.hold) {
      const [at, extra] = flipbook.hold;
      if (frame > at + extra) frame -= extra;
      else if (frame > at) frame = at;
    }
    const boomerang = String(flipbook.Boomerang) === "True";
    const cycle = boomerang ? Math.max(1, count * 2 - 2) : count;
    const repeats = flipbook.rep === -1 ? Number.POSITIVE_INFINITY : flipbook.rep || 1;
    if (frame >= cycle * repeats) return boomerang ? 0 : count - 1;
    const local = frame % cycle;
    return boomerang && local >= count ? cycle - local : local;
  }

  function laurelPose(leafSet, easing, start, hide, t) {
    const enter = (u) => bezier(...easing.in, clamp01(u));
    const leave = (u) => bezier(...easing.out, clamp01(u));
    const entered = t - start;
    const left = t - hide;
    const stemSpec = leafSet.stem;
    let stemScale = entered <= 0 ? 0 : enter(entered / stemSpec.in[1]);
    let stemOpacity = entered >= stemSpec.in[0] ? 1 : 0;
    if (left > 0) {
      stemScale = 1 - leave(left / stemSpec.out[1]);
      if (left >= stemSpec.out[0]) stemOpacity = 0;
    }
    const leaves = leafSet.leaves.map((leaf) => {
      const p = enter((entered - leaf.in[0]) / (leaf.in[1] - leaf.in[0]));
      let rot = leaf.r0 + (leaf.r1 - leaf.r0) * p;
      let scale = leaf.s1 * p;
      let op = 1;
      if (left > 0) {
        const q = leave((left - leaf.out[0]) / (leaf.out[1] - leaf.out[0]));
        rot = leaf.r1 + (leaf.r0 - leaf.r1) * q;
        scale = leaf.s1 * (1 - q);
        if (left >= leaf.hide) op = 0;
      }
      return { rot, scale, op };
    });
    return { stem: { scale: stemScale, op: stemOpacity }, leaves };
  }

  function px(value, width) {
    if (Array.isArray(value)) return value[0] * width + value[1];
    return value;
  }

  function buildLaurel(svg, leafSet, colors) {
    const gradientId = `aw-laurel-${Math.random().toString(36).slice(2, 9)}`;
    const defs = document.createElementNS(SVG, "defs");
    const gradient = document.createElementNS(SVG, "linearGradient");
    for (const [key, value] of Object.entries({ id: gradientId, x1: 1, y1: 0, x2: 1, y2: 1 }))
      gradient.setAttribute(key, value);
    for (const [offset, opacity] of [
      [0, ".6"],
      [1, ".2"],
    ]) {
      const stop = document.createElementNS(SVG, "stop");
      stop.setAttribute("offset", offset);
      stop.setAttribute("stop-color", "#fff");
      stop.setAttribute("stop-opacity", opacity);
      gradient.append(stop);
    }
    defs.append(gradient);
    svg.append(defs);
    const parts = [];
    const half = (group, highlight) => {
      const stem = document.createElementNS(SVG, "path");
      stem.setAttribute("d", leafSet.stem.d);
      stem.setAttribute("fill", "none");
      stem.setAttribute("stroke-width", leafSet.stem.w);
      stem.setAttribute("stroke-linecap", "round");
      if (highlight) stem.setAttribute("stroke", "rgba(255,255,255,.2)");
      group.append(stem);
      parts.push({ node: stem, stem: true });
      leafSet.leaves.forEach((leaf, index) => {
        const path = document.createElementNS(SVG, "path");
        path.setAttribute("d", leaf.d);
        path.setAttribute("stroke", "none");
        if (highlight) path.setAttribute("fill", `url(#${gradientId})`);
        group.append(path);
        parts.push({ node: path, leaf, index });
      });
    };
    for (const highlight of [false, true]) {
      for (const mirrored of [false, true]) {
        const group = document.createElementNS(SVG, "g");
        if (!highlight) {
          group.setAttribute("fill", colors.accent);
          group.setAttribute("stroke", colors.accent);
        }
        if (mirrored) group.setAttribute("transform", "translate(112 0) scale(-1 1)");
        half(group, highlight);
        svg.append(group);
      }
    }
    return parts;
  }

  function drawLaurel(parts, leafSet, pose) {
    const { cx, cy } = leafSet.stem;
    for (const part of parts) {
      if (part.stem) {
        part.node.setAttribute(
          "transform",
          `translate(${cx} ${cy}) scale(${pose.stem.scale.toFixed(4)}) translate(${-cx} ${-cy})`,
        );
        part.node.style.opacity = String(pose.stem.op);
        continue;
      }
      const state = pose.leaves[part.index];
      const leaf = part.leaf;
      part.node.setAttribute(
        "transform",
        `translate(${leaf.tx} ${leaf.ty}) rotate(${state.rot.toFixed(2)} ${leaf.cx} ${leaf.cy}) translate(${leaf.cx} ${leaf.cy}) scale(${state.scale.toFixed(4)}) translate(${-leaf.cx} ${-leaf.cy})`,
      );
      part.node.style.opacity = String(state.op);
    }
  }

  function textWidth(text, weight, size) {
    const context = document.createElement("canvas").getContext("2d");
    if (!context) return 0;
    context.font = `${weight} ${size}px ${FONT}`;
    return context.measureText(text).width;
  }

  function mount(root, options) {
    const { spec, layout, base } = options;
    const images = new Map();
    const image = (file) => {
      if (!images.has(file)) {
        const img = new Image();
        img.src = base + file;
        images.set(file, img);
      }
      return images.get(file);
    };
    for (const panel of Object.values(layout.panels))
      for (const file of Object.values(panel.sheets)) image(file);
    let frame = 0;
    let finished = null;

    function build(kind, lines, colors) {
      const panel = layout.panels[kind];
      const width = measureWidth(
        [
          textWidth(lines[0], 600, layout.toast.fontSize),
          textWidth(lines[1], 350, layout.toast.fontSize),
          textWidth(lines[2], 350, layout.toast.fontSize),
        ],
        layout.toast,
      );
      root.replaceChildren();
      const stage = document.createElement("div");
      stage.className = "aw-xbox-stage";
      stage.style.width = `${layout.stage.width}px`;
      stage.style.height = `${layout.stage.height}px`;
      const named = {};
      const canvases = [];
      let laurel = null;
      const create = (node, parent) => {
        const element =
          node.tag === "svg"
            ? document.createElementNS(SVG, "svg")
            : document.createElement(node.tag);
        if (node.style)
          for (const [key, value] of Object.entries(node.style))
            element.style.setProperty(key, value);
        const box = node.box || {};
        if (box.inset === 0) element.style.inset = "0px";
        for (const key of ["left", "top", "right", "width", "height"]) {
          if (box[key] == null) continue;
          if (node.tag === "canvas" && (key === "width" || key === "height")) {
            element.setAttribute(key, String(px(box[key], width)));
            element.style.setProperty(key, `${px(box[key], width)}px`);
          } else if (node.tag === "svg" && (key === "width" || key === "height")) {
            element.setAttribute(key, String(px(box[key], width)));
          } else {
            element.style.setProperty(key, `${px(box[key], width)}px`);
          }
        }
        if (node.fill) element.style.background = colors[node.fill];
        if (node.backgroundImage)
          element.style.background = `url("${base}${node.backgroundImage}") 0 0 / 100% 100% no-repeat`;
        if (node.src) {
          element.src = base + node.src;
          element.alt = "";
          element.draggable = false;
        }
        if (node.mirror) element.style.transform = "scaleX(-1)";
        if (node.text)
          element.textContent = lines[{ line1: 0, line2: 1, line3: 2 }[node.text]] || "";
        if (node.flipbook)
          canvases.push({
            name: node.flipbook,
            canvas: element,
            spec: spec[panel.panel].flipbooks[node.flipbook],
          });
        if (node.laurel && panel.laurel) {
          element.setAttribute("viewBox", "0 0 112 112");
          element.style.overflow = "visible";
          const leafSet = spec.laurel[panel.laurel];
          laurel = { leafSet, parts: buildLaurel(element, leafSet, colors) };
        }
        if (node.name) named[node.name] = element;
        parent.append(element);
        for (const child of node.children || []) create(child, element);
      };
      const toast = document.createElement("div");
      toast.style.position = "absolute";
      toast.style.left = `${(layout.stage.width - width) / 2}px`;
      toast.style.top = `${layout.stage.toastTop}px`;
      toast.style.width = `${width}px`;
      toast.style.height = `${layout.toast.height}px`;
      for (const child of panel.toast.children) create(child, toast);
      if (panel.toast.style)
        for (const [key, value] of Object.entries(panel.toast.style))
          toast.style.setProperty(key, value);
      toast.style.position = "absolute";
      stage.append(toast);
      root.append(stage);
      return { panel, width, named, canvases, laurel };
    }

    function render(scene, timeline, t) {
      const values = timeline.step(t);
      for (const [name, element] of Object.entries(scene.named)) {
        const v = values[name];
        if (!v) continue;
        element.style.transform = `translate(${v.tx}px, ${v.ty}px) rotate(${v.rot}deg) scale(${v.sx}, ${v.sy})`;
        element.style.opacity = String(clamp01(v.op));
        element.style.visibility = v.vis >= 0.5 ? "" : "hidden";
      }
      const starts = {
        Intro: 0,
        Ambient: scene.panel.ambientStart,
        Exit: scene.hide,
        Aurora: 0,
      };
      for (const book of scene.canvases) {
        const phase = book.name.startsWith("Aurora")
          ? "Aurora"
          : book.name.includes("Intro")
            ? "Intro"
            : book.name.includes("Ambient")
              ? "Ambient"
              : "Exit";
        const sheet = image(scene.panel.sheets[book.name]);
        const context = book.canvas.getContext("2d");
        if (!context || !sheet.complete || !sheet.naturalWidth) continue;
        const elapsed = t - starts[phase];
        const index = elapsed < 0 ? 0 : frameIndex(book.spec, elapsed);
        const frameWidth = Number(book.spec.FrameWidth);
        const frameHeight = Number(book.spec.FrameHeight);
        const columns = Math.max(1, Math.floor(sheet.naturalWidth / frameWidth));
        context.clearRect(0, 0, book.canvas.width, book.canvas.height);
        context.drawImage(
          sheet,
          (index % columns) * frameWidth,
          Math.floor(index / columns) * frameHeight,
          frameWidth,
          frameHeight,
          0,
          0,
          book.canvas.width,
          book.canvas.height,
        );
      }
      if (scene.laurel) {
        drawLaurel(
          scene.laurel.parts,
          scene.laurel.leafSet,
          laurelPose(
            scene.laurel.leafSet,
            layout.laurelEase,
            scene.panel.seq.laurel ?? 0,
            scene.hide,
            t,
          ),
        );
      }
    }

    function stop() {
      if (frame) cancelAnimationFrame(frame);
      frame = 0;
      if (finished) finished();
      finished = null;
    }

    function play(kind, lines, accent) {
      stop();
      const colors = accentShades(accent, layout.accent);
      const scene = build(kind, lines, colors);
      scene.hide = hideTime(spec, scene.panel);
      const timeline = createTimeline(spec, layout, kind, scene.width);
      const end = scene.hide + 2.2;
      const started = performance.now();
      return new Promise((resolve) => {
        finished = resolve;
        const tick = (now) => {
          const t = (now - started) / 1000;
          render(scene, timeline, Math.min(t, end));
          if (t < end) frame = requestAnimationFrame(tick);
          else stop();
        };
        render(scene, timeline, 0);
        frame = requestAnimationFrame(tick);
      });
    }

    return { play, stop };
  }

  globalThis.AwXbox = { measureWidth, accentShades, createTimeline, laurelPose, frameIndex, mount };
})();
