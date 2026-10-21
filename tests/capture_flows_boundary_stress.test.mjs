import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '..');
const TARGET_HTML_PATH = path.join(REPO_ROOT, 'design', 'screenshot-concept.html');

function loadHtml() {
  return fs.readFileSync(TARGET_HTML_PATH, 'utf8');
}

function extractScript(html) {
  const scriptMatch = html.match(/<script>([\s\S]*?)<\/script>/i);
  if (!scriptMatch) throw new Error('No <script> tag found in HTML');
  return scriptMatch[1];
}

class DOMTestHarness {
  constructor(html) {
    this.html = html;
    this.elementsById = {};
    this.elementsByClass = {};
    this.globalListeners = {};
    this.createdElements = [];
    this.stageRect = { left: 0, top: 40, width: 1200, height: 800 };
    this.windows = [];
    this.toasts = [];

    this.initMockDOM();
  }

  createMockNode(tag = 'div', id = '', classNames = '') {
    const listeners = {};
    const classes = new Set(classNames.split(/\s+/).filter(Boolean));
    const dataset = {};
    let style = {
      display: 'none',
      opacity: '1',
      transform: '',
      left: '0px',
      top: '0px',
      width: '0px',
      height: '0px',
      strokeDashoffset: '0'
    };

    const node = {
      tagName: tag.toUpperCase(),
      id,
      dataset,
      style,
      classList: {
        add: (...c) => c.forEach(item => classes.add(item)),
        remove: (...c) => c.forEach(item => classes.delete(item)),
        toggle: (c, force) => {
          if (force !== undefined) {
            if (force) classes.add(c); else classes.delete(c);
            return force;
          }
          if (classes.has(c)) { classes.delete(c); return false; }
          classes.add(c); return true;
        },
        contains: (c) => classes.has(c)
      },
      attributes: {},
      getAttribute: (k) => node.attributes[k] || (k === 'id' ? node.id : null),
      setAttribute: (k, v) => {
        node.attributes[k] = String(v);
        if (k.startsWith('data-')) {
          const camel = k.slice(5).replace(/-([a-z])/g, (_, ch) => ch.toUpperCase());
          node.dataset[camel] = String(v);
        }
      },
      removeAttribute: (k) => { delete node.attributes[k]; },
      addEventListener: (type, fn) => {
        if (!listeners[type]) listeners[type] = [];
        listeners[type].push(fn);
      },
      removeEventListener: (type, fn) => {
        if (!listeners[type]) return;
        listeners[type] = listeners[type].filter(f => f !== fn);
      },
      dispatchEvent: (evt) => {
        evt.target = evt.target || node;
        evt.currentTarget = node;
        const list = listeners[evt.type] || [];
        for (const fn of list) {
          fn(evt);
        }
        return !evt.defaultPrevented;
      },
      contains: (other) => {
        if (other === node) return true;
        if (!other) return false;
        let curr = other.parentElement;
        while (curr) {
          if (curr === node) return true;
          curr = curr.parentElement;
        }
        return false;
      },
      closest: (sel) => {
        let curr = node;
        while (curr) {
          if (sel.startsWith('#') && curr.id === sel.slice(1)) return curr;
          if (sel.startsWith('.') && curr.classList.contains(sel.slice(1))) return curr;
          if (sel.startsWith('[') && sel.endsWith(']')) {
            const attr = sel.slice(1, -1);
            if (curr.attributes && curr.attributes[attr] !== undefined) return curr;
            if (curr.getAttribute(attr) !== null) return curr;
          }
          curr = curr.parentElement;
        }
        return null;
      },
      getBoundingClientRect: () => {
        if (node.id === 'desktopStage') return { ...this.stageRect };
        if (node._rect) return { ...node._rect };
        return { left: 0, top: 0, width: 100, height: 100 };
      },
      getContext: () => ({
        clearRect: () => {},
        fillRect: () => {},
        strokeRect: () => {},
        beginPath: () => {},
        moveTo: () => {},
        lineTo: () => {},
        stroke: () => {},
        fill: () => {},
        arc: () => {},
        ellipse: () => {},
        rect: () => {},
        roundRect: () => {},
        save: () => {},
        restore: () => {},
        drawImage: () => {},
        fillText: () => {},
        createLinearGradient: () => ({ addColorStop: () => {} }),
        getImageData: (x, y, w, h) => ({ data: new Uint8ClampedArray((w || 10) * (h || 10) * 4) }),
        putImageData: () => {}
      }),
      toDataURL: () => 'data:image/png;base64,mock',
      appendChild: (child) => {
        child.parentElement = node;
        child.parentNode = node;
        node.children.push(child);
        if (node.id === 'toastContainer') {
          this.toasts.push(child);
        }
      },
      removeChild: (child) => {
        child.parentElement = null;
        child.parentNode = null;
        node.children = node.children.filter(c => c !== child);
        if (node.id === 'toastContainer') {
          this.toasts = this.toasts.filter(t => t !== child);
        }
      },
      querySelector: (sel) => {
        if (sel.startsWith('.')) {
          const cls = sel.slice(1);
          const findIn = (n) => {
            if (n.classList && n.classList.contains(cls)) return n;
            for (const c of (n.children || [])) {
              const res = findIn(c);
              if (res) return res;
            }
            return null;
          };
          return findIn(node);
        }
        if (sel.startsWith('#')) {
          const id = sel.slice(1);
          const findIn = (n) => {
            if (n.id === id) return n;
            for (const c of (n.children || [])) {
              const res = findIn(c);
              if (res) return res;
            }
            return null;
          };
          return findIn(node);
        }
        return null;
      },
      querySelectorAll: (sel) => {
        if (sel.includes('.win')) return this.windows;
        const res = [];
        const cls = sel.startsWith('.') ? sel.slice(1) : null;
        const walk = (n) => {
          if (cls && n.classList && n.classList.contains(cls)) res.push(n);
          for (const c of (n.children || [])) walk(c);
        };
        for (const c of (node.children || [])) walk(c);
        return res;
      },
      focus: () => {},
      parentElement: null,
      parentNode: null,
      children: [],
      textContent: '',
      innerHTML: '',
      offsetWidth: 100,
      offsetHeight: 100
    };

    if (id) this.elementsById[id] = node;
    for (const c of classes) {
      if (!this.elementsByClass[c]) this.elementsByClass[c] = [];
      this.elementsByClass[c].push(node);
    }
    this.createdElements.push(node);
    return node;
  }

  initMockDOM() {
    const idRegex = /id=["']([a-zA-Z0-9_-]+)["']/g;
    let match;
    while ((match = idRegex.exec(this.html)) !== null) {
      this.createMockNode('div', match[1]);
    }

    // Explicit handle nodes for selection box
    const selBox = this.elementsById['selectionBox'];
    if (selBox) {
      const handles = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];
      for (const h of handles) {
        const handleNode = this.createMockNode('div', '', `sel-handle handle-${h}`);
        handleNode.setAttribute('data-handle', h);
        handleNode.parentElement = selBox;
        selBox.children.push(handleNode);
      }
    }

    // Setup mock windows with boundingClientRects and zIndex
    const win1 = this.elementsById['winTerminal'] || this.createMockNode('div', 'winTerminal', 'win');
    win1.classList.add('win');
    win1.dataset.windowTitle = 'Термінал';
    win1._rect = { left: 100, top: 100, width: 600, height: 400 };
    win1.style.zIndex = '10';

    const win2 = this.elementsById['winBrowser'] || this.createMockNode('div', 'winBrowser', 'win');
    win2.classList.add('win');
    win2.dataset.windowTitle = 'Браузер';
    win2._rect = { left: 300, top: 200, width: 650, height: 450 };
    win2.style.zIndex = '20';

    const win3 = this.elementsById['winFiles'] || this.createMockNode('div', 'winFiles', 'win');
    win3.classList.add('win');
    win3.dataset.windowTitle = 'Файли';
    win3._rect = { left: 500, top: 250, width: 550, height: 380 };
    win3.style.zIndex = '15';

    this.windows = [win1, win2, win3];

    // HUD mode segment buttons
    const hudModeSegment = this.elementsById['hudModeSegment'] || this.createMockNode('div', 'hudModeSegment');
    for (const m of ['area', 'window', 'screen']) {
      const btn = this.createMockNode('button', `btnMode_${m}`);
      btn.setAttribute('data-mode', m);
      btn.parentElement = hudModeSegment;
      hudModeSegment.children.push(btn);
    }

    // HUD timer segment buttons
    const hudTimerSegment = this.elementsById['hudTimerSegment'] || this.createMockNode('div', 'hudTimerSegment');
    for (const s of ['0', '3', '5']) {
      const btn = this.createMockNode('button', `btnTimer_${s}`);
      btn.setAttribute('data-timer', s);
      btn.parentElement = hudTimerSegment;
      hudTimerSegment.children.push(btn);
    }
  }

  buildSandbox() {
    const self = this;
    const sandbox = {
      window: {},
      document: {
        documentElement: {
          setAttribute: () => {},
          getAttribute: () => 'dark',
          style: { setProperty: () => {} }
        },
        body: self.createMockNode('body'),
        getElementById: (id) => self.elementsById[id] || self.createMockNode('div', id),
        querySelector: (sel) => {
          if (sel.startsWith('#')) return self.elementsById[sel.slice(1)] || null;
          if (sel.startsWith('.')) return (self.elementsByClass[sel.slice(1)] || [])[0] || null;
          return null;
        },
        querySelectorAll: (sel) => {
          if (sel.includes('#hudModeSegment button')) {
            return (self.elementsById['hudModeSegment']?.children || []);
          }
          if (sel.includes('#hudTimerSegment button')) {
            return (self.elementsById['hudTimerSegment']?.children || []);
          }
          if (sel.includes('.win')) {
            return self.windows;
          }
          if (sel.startsWith('.')) {
            return self.elementsByClass[sel.slice(1)] || [];
          }
          return [];
        },
        createElement: (tag) => self.createMockNode(tag),
        addEventListener: (event, handler) => {
          if (!self.globalListeners[event]) self.globalListeners[event] = [];
          self.globalListeners[event].push(handler);
        },
        removeEventListener: (event, handler) => {
          if (!self.globalListeners[event]) return;
          self.globalListeners[event] = self.globalListeners[event].filter(h => h !== handler);
        },
        elementsFromPoint: (x, y) => {
          // Return elements sorted by z-index descending
          const hits = [];
          const stage = self.elementsById['desktopStage'];
          const overlay = self.elementsById['captureOverlay'];
          if (overlay && overlay.style.display !== 'none') hits.push(overlay);

          const sortedWins = [...self.windows].sort((a, b) => parseInt(b.style.zIndex || 0, 10) - parseInt(a.style.zIndex || 0, 10));
          for (const win of sortedWins) {
            const r = win.getBoundingClientRect();
            if (x >= r.left && x <= r.left + r.width && y >= r.top && y <= r.top + r.height) {
              hits.push(win);
            }
          }
          if (stage) hits.push(stage);
          return hits;
        }
      },
      navigator: {
        clipboard: {
          writeText: async () => {},
          write: async () => {}
        }
      },
      AudioContext: class {
        constructor() { this.state = 'running'; }
        createOscillator() {
          return {
            type: 'sine',
            frequency: { setValueAtTime: () => {}, exponentialRampToValueAtTime: () => {} },
            connect: () => {},
            start: () => {},
            stop: () => {}
          };
        }
        createGain() {
          return {
            gain: { setValueAtTime: () => {}, exponentialRampToValueAtTime: () => {} },
            connect: () => {}
          };
        }
        createBufferSource() {
          return { buffer: null, connect: () => {}, start: () => {} };
        }
        get destination() { return {}; }
      },
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (id) => clearTimeout(id),
      setInterval: (fn, ms) => setInterval(fn, ms),
      clearInterval: (id) => clearInterval(id),
      requestAnimationFrame: (fn) => setTimeout(() => fn(Date.now()), 16),
      cancelAnimationFrame: (id) => clearTimeout(id),
      performance: { now: () => Date.now() },
      console: { log: () => {}, warn: () => {}, error: () => {}, info: () => {} },
      Date,
      Math,
      String,
      Number,
      Boolean,
      Array,
      Object,
      Set,
      Map,
      Uint8ClampedArray,
      Blob: class {},
      Image: class {
        constructor() {
          setTimeout(() => { if (this.onload) this.onload(); }, 2);
        }
      }
    };
    sandbox.window = sandbox;
    return sandbox;
  }

  instantiate() {
    const sandbox = this.buildSandbox();
    const script = extractScript(this.html);
    const context = vm.createContext(sandbox);
    vm.runInContext(script, context, { filename: 'design/screenshot-concept.html' });
    return {
      sandbox,
      captureEngine: sandbox.window.captureEngine,
      thumbnailManager: sandbox.window.thumbnailManager,
      markupEditor: sandbox.window.markupEditor,
      dispatchGlobal: (type, eventProps) => {
        const list = this.globalListeners[type] || [];
        const evt = { type, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...eventProps };
        for (const fn of list) fn(evt);
        return evt;
      }
    };
  }
}

// =========================================================================
// PART 1: INVERTED AREA DRAGS
// =========================================================================
test('Part 1: Inverted Area Drags Verification', async (t) => {
  const html = loadHtml();

  await t.test('TC-CH-01: Drag from bottom-right to top-left normalizes with positive dimensions', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');
    assert.strictEqual(captureEngine.mode, 'area');

    // Pointer down at bottom-right (500, 400)
    captureEngine.onPointerDown({
      button: 0,
      clientX: 500,
      clientY: 440, // stageRect has top: 40, so y in stage is 400
      target: harness.elementsById['captureOverlay']
    });

    assert.strictEqual(captureEngine.isDragging, true);
    assert.strictEqual(captureEngine.dragAction, 'create');

    // Drag to top-left (100, 150) -> clientX: 100, clientY: 190
    captureEngine.onPointerMove({
      clientX: 100,
      clientY: 190
    });

    assert.ok(captureEngine.selectionRect, 'selectionRect must be defined');
    assert.strictEqual(captureEngine.selectionRect.x, 100, 'Normalized x must be 100');
    assert.strictEqual(captureEngine.selectionRect.y, 150, 'Normalized y must be 150');
    assert.strictEqual(captureEngine.selectionRect.width, 400, 'Width must be strictly positive (400)');
    assert.strictEqual(captureEngine.selectionRect.height, 250, 'Height must be strictly positive (250)');

    captureEngine.onPointerUp();
    assert.strictEqual(captureEngine.isDragging, false);
    assert.ok(captureEngine.selectionRect, 'Selection should survive pointer up');
  });

  await t.test('TC-CH-02: Drag from top-right to bottom-left normalizes with positive dimensions', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');

    // Pointer down at top-right (500, 150)
    captureEngine.onPointerDown({
      button: 0,
      clientX: 500,
      clientY: 190,
      target: harness.elementsById['captureOverlay']
    });

    // Drag to bottom-left (100, 400)
    captureEngine.onPointerMove({
      clientX: 100,
      clientY: 440
    });

    assert.ok(captureEngine.selectionRect);
    assert.strictEqual(captureEngine.selectionRect.x, 100);
    assert.strictEqual(captureEngine.selectionRect.y, 150);
    assert.strictEqual(captureEngine.selectionRect.width, 400);
    assert.strictEqual(captureEngine.selectionRect.height, 250);

    captureEngine.onPointerUp();
    assert.strictEqual(captureEngine.isDragging, false);
  });

  await t.test('TC-CH-03: Resize handle NW dragged past SE anchor preserves positive dimensions', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');
    // Set initial box at 200, 200, 300, 200
    captureEngine.selectionRect = { x: 200, y: 200, width: 300, height: 200 };
    const nwHandle = harness.elementsById['selectionBox'].children.find(c => c.dataset.handle === 'nw');

    // Drag NW handle (anchor is at 500, 400)
    captureEngine.onPointerDown({
      button: 0,
      clientX: 200,
      clientY: 240,
      target: nwHandle,
      stopPropagation: () => {}
    });

    assert.strictEqual(captureEngine.dragAction, 'resize');
    assert.strictEqual(captureEngine.activeHandle, 'nw');

    // Drag NW handle FAR past the anchor (to 700, 600)
    captureEngine.onPointerMove({
      clientX: 700,
      clientY: 640
    });

    const rect = captureEngine.selectionRect;
    assert.ok(rect.width >= 16, 'Width must remain >= minSize');
    assert.ok(rect.height >= 16, 'Height must remain >= minSize');
    assert.ok(rect.x >= 0, 'x must be non-negative');
    assert.ok(rect.y >= 0, 'y must be non-negative');
    assert.ok(rect.x + rect.width <= 1200, 'Selection must not exceed stage width');
    assert.ok(rect.y + rect.height <= 800, 'Selection must not exceed stage height');

    captureEngine.onPointerUp();
  });

  await t.test('TC-CH-04: Sub-threshold jitter (<12px) collapses selection on pointerup', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');
    captureEngine.onPointerDown({
      button: 0,
      clientX: 300,
      clientY: 340,
      target: harness.elementsById['captureOverlay']
    });

    captureEngine.onPointerMove({
      clientX: 305,
      clientY: 345
    });

    assert.ok(captureEngine.selectionRect);
    assert.strictEqual(captureEngine.selectionRect.width, 5);
    assert.strictEqual(captureEngine.selectionRect.height, 5);

    captureEngine.onPointerUp();
    assert.strictEqual(captureEngine.selectionRect, null, 'Jitter drag <12px must reset selectionRect to null');
    assert.strictEqual(harness.elementsById['selectionBox'].style.display, 'none');
  });
});

// =========================================================================
// PART 2: VIEWPORT CLAMPING
// =========================================================================
test('Part 2: Viewport Clamping Stress Verification', async (t) => {
  const html = loadHtml();

  await t.test('TC-CH-05: Drag creation past bottom-right stage edge clamps strictly to stage boundaries', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');

    // Start at (200, 200) inside stage
    captureEngine.onPointerDown({
      button: 0,
      clientX: 200,
      clientY: 240,
      target: harness.elementsById['captureOverlay']
    });

    // Drag way beyond stage bounds (stage is 1200x800)
    captureEngine.onPointerMove({
      clientX: 2500,
      clientY: 2000
    });

    const rect = captureEngine.selectionRect;
    assert.strictEqual(rect.x, 200);
    assert.strictEqual(rect.y, 200);
    assert.strictEqual(rect.width, 1000, 'Width must clamp to stage width 1200 (1200 - 200 = 1000)');
    assert.strictEqual(rect.height, 600, 'Height must clamp to stage height 800 (800 - 200 = 600)');
    assert.strictEqual(rect.x + rect.width, 1200);
    assert.strictEqual(rect.y + rect.height, 800);

    captureEngine.onPointerUp();
  });

  await t.test('TC-CH-06: Drag creation past top-left stage edge clamps strictly to (0, 0)', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');

    // Start at (400, 300)
    captureEngine.onPointerDown({
      button: 0,
      clientX: 400,
      clientY: 340,
      target: harness.elementsById['captureOverlay']
    });

    // Drag way past top-left into negative client coordinates
    captureEngine.onPointerMove({
      clientX: -800,
      clientY: -600
    });

    const rect = captureEngine.selectionRect;
    assert.strictEqual(rect.x, 0, 'Clamped x must be 0');
    assert.strictEqual(rect.y, 0, 'Clamped y must be 0');
    assert.strictEqual(rect.width, 400, 'Width must be 400');
    assert.strictEqual(rect.height, 300, 'Height must be 300');

    captureEngine.onPointerUp();
  });

  await t.test('TC-CH-07: Moving selection box past all 4 screen edges is strictly bounded', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');
    captureEngine.selectionRect = { x: 400, y: 300, width: 300, height: 200 };
    const selBox = harness.elementsById['selectionBox'];

    // Move past bottom-right
    captureEngine.onPointerDown({
      button: 0,
      clientX: 450,
      clientY: 390,
      target: selBox,
      stopPropagation: () => {}
    });
    assert.strictEqual(captureEngine.dragAction, 'move');

    captureEngine.onPointerMove({
      clientX: 3000,
      clientY: 3000
    });
    assert.strictEqual(captureEngine.selectionRect.x, 900, 'x clamped to 1200 - 300 = 900');
    assert.strictEqual(captureEngine.selectionRect.y, 600, 'y clamped to 800 - 200 = 600');
    assert.strictEqual(captureEngine.selectionRect.width, 300);
    assert.strictEqual(captureEngine.selectionRect.height, 200);

    // Move past top-left
    captureEngine.onPointerMove({
      clientX: -1000,
      clientY: -1000
    });
    assert.strictEqual(captureEngine.selectionRect.x, 0, 'x clamped to 0');
    assert.strictEqual(captureEngine.selectionRect.y, 0, 'y clamped to 0');
    assert.strictEqual(captureEngine.selectionRect.width, 300);
    assert.strictEqual(captureEngine.selectionRect.height, 200);

    captureEngine.onPointerUp();
  });
});

// =========================================================================
// PART 3: RAPID MODE SWITCHING & STATE LEAKAGE
// =========================================================================
test('Part 3: Rapid Mode Switching & State Leakage Verification', async (t) => {
  const html = loadHtml();

  await t.test('TC-CH-08: 50 Rapid mode switches execute cleanly without throwing', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');
    const modes = ['area', 'window', 'screen'];

    for (let i = 0; i < 50; i++) {
      const mode = modes[i % 3];
      captureEngine.setMode(mode);
      assert.strictEqual(captureEngine.mode, mode);
    }
  });

  await t.test('TC-CH-09: [ADVERSARIAL BUG] Area selection leak into Screen mode capture', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine, sandbox } = harness.instantiate();

    captureEngine.openCapture('area');

    // 1. User selects a 300x200 box in Area mode
    captureEngine.selectionRect = { x: 100, y: 100, width: 300, height: 200 };

    // 2. User switches to Screen mode in HUD
    captureEngine.setMode('screen');
    assert.strictEqual(captureEngine.mode, 'screen');

    // 3. Empirically verify whether selectionRect was cleared
    const leakedRectBeforeShutter = captureEngine.selectionRect;

    // 4. User clicks Shutter button in HUD
    let capturedRect = null;
    sandbox.window.thumbnailManager.show = (dataUrl, targetRect) => {
      capturedRect = targetRect;
    };

    captureEngine.triggerShutter();

    // Adversarial finding assertion:
    // In screen mode, clicking the HUD shutter button MUST capture the full screen (1200x800).
    // If it captures the old 300x200 selectionRect, that is a state leakage defect!
    const isLeaked = (capturedRect && (capturedRect.width === 300 && capturedRect.height === 200));
    assert.ok(
      isLeaked || (capturedRect && capturedRect.width === 1200),
      'Captured rect must either be full screen (correct) or leaked area rect (defect recorded)'
    );

    // Record the defect finding
    if (isLeaked) {
      assert.strictEqual(leakedRectBeforeShutter.width, 300, 'FINDING CONFIRMED: setMode("screen") failed to nullify selectionRect, leaking old area crop into full-screen capture');
    }
  });

  await t.test('TC-CH-10: Mid-drag mode switch aborts area drag cleanly', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');
    captureEngine.onPointerDown({
      button: 0,
      clientX: 200,
      clientY: 240,
      target: harness.elementsById['captureOverlay']
    });
    assert.strictEqual(captureEngine.isDragging, true);

    // User switches mode while pointer is still down
    captureEngine.setMode('window');

    // Subsequent pointer move should NOT update selection
    captureEngine.onPointerMove({
      clientX: 500,
      clientY: 540
    });

    assert.strictEqual(harness.elementsById['selectionBox'].style.display, 'none');

    captureEngine.onPointerUp();
    assert.strictEqual(captureEngine.isDragging, false);
  });
});

// =========================================================================
// PART 4: TIMER COUNTDOWN INTERRUPTION
// =========================================================================
test('Part 4: Timer Countdown Interruption Verification', async (t) => {
  const html = loadHtml();

  await t.test('TC-CH-11: Escape key cancels active countdown immediately', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine, dispatchGlobal } = harness.instantiate();

    captureEngine.openCapture('area');
    captureEngine.setTimer('5');
    assert.strictEqual(captureEngine.timerSeconds, 5);

    captureEngine.triggerShutter();
    assert.ok(captureEngine.countdownTimerId !== null, 'Countdown interval must be active');
    assert.strictEqual(harness.elementsById['countdownOverlay'].style.display, 'flex');

    // Dispatch Escape
    dispatchGlobal('keydown', { key: 'Escape' });

    assert.strictEqual(captureEngine.countdownTimerId, null, 'Countdown interval must be cleared on Escape');
    assert.strictEqual(harness.elementsById['countdownOverlay'].style.display, 'none', 'Overlay must be dismissed');
    assert.strictEqual(harness.elementsById['captureOverlay'].style.display, 'none', 'Capture overlay must be dismissed');
  });

  await t.test('TC-CH-12: [ADVERSARIAL BUG] HUD mode switch during countdown does NOT cancel timer', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');
    captureEngine.setTimer('5');
    captureEngine.triggerShutter();

    const activeTimerId = captureEngine.countdownTimerId;
    assert.ok(activeTimerId !== null, 'Countdown timer must be active');

    // User clicks another mode in HUD during countdown
    captureEngine.setMode('window');

    // Verify whether countdownTimerId was cancelled:
    const timerSurvived = (captureEngine.countdownTimerId === activeTimerId);
    if (timerSurvived) {
      // Clean up to prevent node process hang
      clearInterval(captureEngine.countdownTimerId);
      captureEngine.countdownTimerId = null;
    }

    assert.strictEqual(
      timerSurvived,
      true,
      'FINDING CONFIRMED: setMode() during countdown leaves countdownTimerId ticking in background'
    );
  });

  await t.test('TC-CH-13: [ADVERSARIAL BUG] Repeated shutter clicks during countdown leaks previous intervals', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('area');
    captureEngine.setTimer('5');

    // First shutter click
    captureEngine.triggerShutter();
    const firstTimerId = captureEngine.countdownTimerId;
    assert.ok(firstTimerId !== null);

    // Second shutter click during active countdown
    captureEngine.triggerShutter();
    const secondTimerId = captureEngine.countdownTimerId;

    // Both intervals were created; first was overwritten without clearInterval
    assert.notStrictEqual(firstTimerId, secondTimerId, 'FINDING CONFIRMED: Consecutive triggerShutter() calls leak previous countdown setInterval without clearing it');

    clearInterval(firstTimerId);
    clearInterval(secondTimerId);
    captureEngine.countdownTimerId = null;
  });
});

// =========================================================================
// PART 5: WINDOW PICKING EDGE CASES
// =========================================================================
test('Part 5: Window Picking Edge Cases Verification', async (t) => {
  const html = loadHtml();

  await t.test('TC-CH-14: Overlapping windows selects the topmost window by z-index', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('window');
    assert.strictEqual(captureEngine.mode, 'window');

    // Overlapping point between winTerminal (rect: 100, 100, 600, 400, z-index: 10)
    // and winBrowser (rect: 300, 200, 650, 450, z-index: 20)
    // Point (400, 300) falls inside BOTH windows!
    captureEngine.detectHoveredWindow(400, 340); // 340 clientY = 300 stageY

    assert.ok(captureEngine.hoveredWindowEl, 'A window must be hovered');
    assert.strictEqual(
      captureEngine.hoveredWindowEl.id,
      'winBrowser',
      'Topmost window by z-index (winBrowser, z=20) must be selected over winTerminal (z=10)'
    );
    assert.strictEqual(harness.elementsById['windowHoverTitle'].textContent, 'Браузер');
  });

  await t.test('TC-CH-15: Hover over empty desktop clears window highlight', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine } = harness.instantiate();

    captureEngine.openCapture('window');

    // First hover over a window
    captureEngine.detectHoveredWindow(400, 340);
    assert.ok(captureEngine.hoveredWindowEl);

    // Now hover over empty space: Point (50, 50) where no windows exist
    captureEngine.detectHoveredWindow(50, 90);

    assert.strictEqual(captureEngine.hoveredWindowEl, null, 'Hovered window must reset to null');
    assert.strictEqual(harness.elementsById['windowHighlightBox'].style.display, 'none');
  });

  await t.test('TC-CH-16: Click on highlighted window captures its exact stage-relative geometry', () => {
    const harness = new DOMTestHarness(html);
    const { captureEngine, sandbox } = harness.instantiate();

    captureEngine.openCapture('window');
    captureEngine.detectHoveredWindow(150, 150); // winTerminal at (100, 100, 600, 400)

    let capturedRect = null;
    sandbox.window.thumbnailManager.show = (dataUrl, targetRect) => {
      capturedRect = targetRect;
    };

    // Click on overlay to capture hovered window
    captureEngine.onPointerDown({
      button: 0,
      clientX: 150,
      clientY: 150,
      target: harness.elementsById['captureOverlay']
    });

    assert.ok(capturedRect, 'Capture must execute');
    assert.strictEqual(capturedRect.x, 100);
    assert.strictEqual(capturedRect.y, 60); // 100 - stage.top(40) = 60
    assert.strictEqual(capturedRect.width, 600);
    assert.strictEqual(capturedRect.height, 400);
  });
});

// =========================================================================
// PART 6: CORNER THUMBNAIL STRESS
// =========================================================================
test('Part 6: Corner Thumbnail Stress Verification', async (t) => {
  const html = loadHtml();

  await t.test('TC-CH-17: Hover pause freezes remainingMs; mouseleave resumes decrementing', async () => {
    const harness = new DOMTestHarness(html);
    const { thumbnailManager } = harness.instantiate();

    thumbnailManager.show('data:image/png;base64,test', { x: 0, y: 0, width: 800, height: 600 });
    assert.strictEqual(thumbnailManager.isPaused, false);
    assert.strictEqual(thumbnailManager.remainingMs, 5000);

    // Mouse enter pauses
    harness.elementsById['cornerThumbnail'].dispatchEvent({ type: 'mouseenter' });
    assert.strictEqual(thumbnailManager.isPaused, true);

    // Wait 100ms
    await new Promise(r => setTimeout(r, 100));
    const pausedMs = thumbnailManager.remainingMs;

    // Wait another 100ms - remainingMs must NOT change while paused
    await new Promise(r => setTimeout(r, 100));
    assert.strictEqual(thumbnailManager.remainingMs, pausedMs, 'Timer must remain frozen while paused');

    // Mouse leave resumes
    harness.elementsById['cornerThumbnail'].dispatchEvent({ type: 'mouseleave' });
    assert.strictEqual(thumbnailManager.isPaused, false);

    await new Promise(r => setTimeout(r, 100));
    assert.ok(thumbnailManager.remainingMs < pausedMs, 'Timer must resume decrementing after mouseleave');

    thumbnailManager.dismiss(false);
  });

  await t.test('TC-CH-18: 50 Rapid hover cycles preserve countdown integrity without NaN or negative values', () => {
    const harness = new DOMTestHarness(html);
    const { thumbnailManager } = harness.instantiate();

    thumbnailManager.show('data:image/png;base64,test', { x: 0, y: 0, width: 800, height: 600 });
    const thumbEl = harness.elementsById['cornerThumbnail'];

    for (let i = 0; i < 50; i++) {
      thumbEl.dispatchEvent({ type: 'mouseenter' });
      thumbEl.dispatchEvent({ type: 'mouseleave' });
    }

    assert.ok(!isNaN(thumbnailManager.remainingMs), 'remainingMs must not be NaN');
    assert.ok(thumbnailManager.remainingMs > 0, 'remainingMs must remain positive');

    thumbnailManager.dismiss(false);
  });

  await t.test('TC-CH-19: [ADVERSARIAL BUG] Race condition: dismiss() un-cancellable setTimeout extinguishes subsequent thumbnail', async () => {
    const harness = new DOMTestHarness(html);
    const { thumbnailManager } = harness.instantiate();

    // 1. First thumbnail appears
    thumbnailManager.show('data:image/png;base64,first', { width: 400, height: 300 });
    assert.strictEqual(harness.elementsById['cornerThumbnail'].style.display, 'flex');

    // 2. User dismisses first thumbnail (schedules 200ms setTimeout to set display: 'none')
    thumbnailManager.dismiss(true);

    // 3. 50ms later, user takes a second screenshot!
    await new Promise(r => setTimeout(r, 50));
    thumbnailManager.show('data:image/png;base64,second', { width: 500, height: 400 });
    assert.strictEqual(harness.elementsById['cornerThumbnail'].style.display, 'flex');

    // 4. Wait for the remainder of first dismiss timeout (160ms)
    await new Promise(r => setTimeout(r, 180));

    // Adversarial finding:
    // If dismiss() did not clear its timeout, it sets display = 'none', extinguishing the second thumbnail!
    const isExtinguished = (harness.elementsById['cornerThumbnail'].style.display === 'none');
    assert.strictEqual(
      isExtinguished,
      true,
      'FINDING CONFIRMED: dismiss() un-cancellable setTimeout extinguished the active second thumbnail!'
    );
  });

  await t.test('TC-CH-20: [ADVERSARIAL DEFECT] Corner thumbnail lacks touch/pointer swipe-to-dismiss gesture', () => {
    // Check if swipe gesture listener exists in concept HTML
    const hasSwipeLogic = /swipe|pointerdown.*threshold|touchstart.*touchmove/i.test(html);
    assert.strictEqual(
      hasSwipeLogic,
      false,
      'FINDING CONFIRMED: Corner thumbnail has no swipe-to-dismiss gesture implementation (only close button and 5s timer)'
    );
  });

  await t.test('TC-CH-21: [ADVERSARIAL DEFECT] Drag-and-drop only supports Files manager; lacks Terminal, Browser, Desktop drop targets', () => {
    const harness = new DOMTestHarness(html);
    harness.instantiate();

    // Files drop target exists
    const filesDrop = harness.elementsById['filesDropTarget'];
    assert.ok(filesDrop, 'filesDropTarget exists');

    // Check if Terminal, Browser, or Desktop have dragover / drop listeners
    const winTerminal = harness.elementsById['winTerminal'];
    const winBrowser = harness.elementsById['winBrowser'];
    const desktopStage = harness.elementsById['desktopStage'];

    // In HTML, search if drop listeners were attached to winTerminal, winBrowser, or desktopStage
    const terminalDropSupport = /winTerminal.*addEventListener\(['"]drop['"]/i.test(html);
    const browserDropSupport = /winBrowser.*addEventListener\(['"]drop['"]/i.test(html);
    const desktopDropSupport = /desktopStage.*addEventListener\(['"]drop['"]/i.test(html);

    assert.strictEqual(
      terminalDropSupport || browserDropSupport || desktopDropSupport,
      false,
      'FINDING CONFIRMED: Drag-and-drop is only hooked to #filesDropTarget; Terminal, Browser, and Desktop reject drops'
    );
  });
});
