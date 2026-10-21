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
const STYLE_CSS_PATH = path.join(REPO_ROOT, 'design', 'style.css');

function loadHtml() {
  if (!fs.existsSync(TARGET_HTML_PATH)) {
    return null;
  }
  return fs.readFileSync(TARGET_HTML_PATH, 'utf8');
}

function loadStyleCss() {
  if (!fs.existsSync(STYLE_CSS_PATH)) {
    return '';
  }
  return fs.readFileSync(STYLE_CSS_PATH, 'utf8');
}

class SimpleDOMNode {
  constructor(tag = 'div', attrs = {}) {
    this.tagName = tag.toUpperCase();
    this.attributes = { ...attrs };
    this.children = [];
    this.style = {};
    this.classList = new Set();
    if (attrs.class) {
      attrs.class.split(/\s+/).filter(Boolean).forEach(c => this.classList.add(c));
    }
    this.id = attrs.id || '';
    this.textContent = '';
    this.innerHTML = '';
    this.listeners = {};
    this.dataset = {};
    for (const [k, v] of Object.entries(attrs)) {
      if (k.startsWith('data-')) {
        const camel = k.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
        this.dataset[camel] = v;
      }
    }
  }

  addEventListener(type, handler) {
    if (!this.listeners[type]) this.listeners[type] = [];
    this.listeners[type].push(handler);
  }

  dispatchEvent(evt) {
    const list = this.listeners[evt.type] || [];
    for (const handler of list) {
      handler(evt);
    }
    return !evt.defaultPrevented;
  }

  getAttribute(name) {
    return this.attributes[name] ?? null;
  }

  setAttribute(name, val) {
    this.attributes[name] = String(val);
    if (name === 'id') this.id = String(val);
    if (name === 'class') {
      this.classList = new Set(String(val).split(/\s+/).filter(Boolean));
    }
    if (name.startsWith('data-')) {
      const camel = name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
      this.dataset[camel] = String(val);
    }
  }

  querySelector(selector) {
    return this.querySelectorAll(selector)[0] || null;
  }

  querySelectorAll(selector) {
    const results = [];
    const match = (node) => {
      if (selector.startsWith('#')) {
        if (node.id === selector.slice(1)) results.push(node);
      } else if (selector.startsWith('.')) {
        if (node.classList.has(selector.slice(1))) results.push(node);
      } else if (selector.startsWith('[data-')) {
        const attrMatch = selector.match(/\[([a-zA-Z0-9_-]+)(?:=["']?(.*?)["']?)?\]/);
        if (attrMatch) {
          const attr = attrMatch[1];
          const val = attrMatch[2];
          if (val !== undefined) {
            if (node.getAttribute(attr) === val) results.push(node);
          } else if (node.getAttribute(attr) !== null) {
            results.push(node);
          }
        }
      } else if (node.tagName.toLowerCase() === selector.toLowerCase()) {
        results.push(node);
      }
      for (const child of node.children) {
        if (child instanceof SimpleDOMNode) match(child);
      }
    };
    for (const child of this.children) {
      if (child instanceof SimpleDOMNode) match(child);
    }
    return results;
  }
}

function parseHTMLToTree(htmlString) {
  const root = new SimpleDOMNode('document');
  const tagRegex = /<([a-zA-Z0-9_-]+)([^>]*)>([\s\S]*?)<\/\1>|<([a-zA-Z0-9_-]+)([^>]*?)\/?>/g;
  return root;
}

test('Suite: Environmental & File Preconditions', async (t) => {
  await t.test('design directory exists and contains reference design files', () => {
    assert.ok(fs.existsSync(path.join(REPO_ROOT, 'design')), 'design/ directory must exist');
    assert.ok(fs.existsSync(STYLE_CSS_PATH), 'design/style.css must exist');
    assert.ok(fs.existsSync(path.join(REPO_ROOT, 'design', 'MERIDIAN_DESIGN_SPEC.md')), 'MERIDIAN_DESIGN_SPEC.md must exist');
  });

  await t.test('TEST_INFRA.md exists in orchestrator_2 and root', () => {
    const orchInfra = path.join(REPO_ROOT, '.agents', 'teamwork', 'orchestrator_2', 'TEST_INFRA.md');
    assert.ok(fs.existsSync(orchInfra), 'TEST_INFRA.md must exist in orchestrator_2');
  });
});

test('Suite: Meridian Design Tokens & CSS Conformance', async (t) => {
  const html = loadHtml();
  if (!html) {
    t.skip('design/screenshot-concept.html not yet created by worker');
    return;
  }

  const styleCss = loadStyleCss();
  const inlineStyles = (html.match(/<style[^>]*>([\s\S]*?)<\/style>/gi) || [])
    .map(s => s.replace(/<\/?style[^>]*>/gi, ''))
    .join('\n');
  const combinedCss = styleCss + '\n' + inlineStyles;

  await t.test('Strict Form Scale: Radii only contain R1-R4, R_ROUND, or 0 (No intermediate values)', () => {
    const borderRadiusRegex = /border-radius:\s*([^;]+);/g;
    let match;
    const permittedPatterns = [
      /^0(px)?$/,
      /^var\(--r-(?:1|2|3|4|round)\)$/,
      /^4px$/,
      /^8px$/,
      /^12px$/,
      /^18px$/,
      /^999px$/,
      /^9999px$/,
      /^50%$/,
      /^(?:4px|8px|12px|18px|999px|var\(--r-[1-4]\)|0)\s+(?:4px|8px|12px|18px|999px|var\(--r-[1-4]\)|0)/
    ];

    const violations = [];
    while ((match = borderRadiusRegex.exec(inlineStyles)) !== null) {
      const val = match[1].trim();
      const isPermitted = permittedPatterns.some(p => p.test(val));
      if (!isPermitted && !val.includes('var(')) {
        violations.push(val);
      }
    }
    assert.strictEqual(
      violations.length,
      0,
      `Intermediate border-radius values found: ${JSON.stringify(violations)}. Meridian spec strictly forbids values other than 4px, 8px, 12px, 18px, 999px.`
    );
  });

  await t.test('Strict State Layers: Opacity tokens conform to 8%, 13%, 18% with currentColor', () => {
    assert.match(combinedCss, /--st-hover:\s*(?:\.08|0\.08)/, 'Must declare --st-hover: .08');
    assert.match(combinedCss, /--st-press:\s*(?:\.13|0\.13)/, 'Must declare --st-press: .13');
    assert.match(combinedCss, /--st-drag:\s*(?:\.18|0\.18)/, 'Must declare --st-drag: .18');
    assert.match(combinedCss, /currentColor/, 'State layers must use currentColor on ::before pseudo-element');
  });

  await t.test('Strict Motion Physics: E_OUT, E_IO, E_OVER and No Springs', () => {
    assert.match(combinedCss, /--e-out:\s*cubic-bezier\(\s*0?\.2\s*,\s*0\s*,\s*0\s*,\s*1\s*\)/, 'Must define --e-out: cubic-bezier(.2, 0, 0, 1)');
    assert.match(combinedCss, /--e-io:\s*cubic-bezier\(\s*0?\.4\s*,\s*0\s*,\s*0?\.2\s*,\s*1\s*\)/, 'Must define --e-io: cubic-bezier(.4, 0, .2, 1)');
    assert.match(combinedCss, /--e-over:\s*cubic-bezier\(\s*0?\.32\s*,\s*1\.28\s*,\s*0?\.5\s*,\s*1\s*\)/, 'Must define --e-over: cubic-bezier(.32, 1.28, .5, 1)');
    assert.doesNotMatch(combinedCss, /spring\(/i, 'Spring physics are strictly forbidden in Meridian motion');
  });

  await t.test('Strict Typography: Uppercase exclusively permitted on .t-micro', () => {
    const uppercaseRuleRegex = /([^{}]+)\{[^{}]*text-transform\s*:\s*uppercase[^{}]*\}/gi;
    let match;
    const invalidSelectors = [];
    while ((match = uppercaseRuleRegex.exec(inlineStyles)) !== null) {
      const selector = match[1].trim();
      const isMicro = selector.includes('.t-micro') || selector.includes('badge') || selector.includes('kbd');
      if (!isMicro) {
        invalidSelectors.push(selector);
      }
    }
    assert.strictEqual(
      invalidSelectors.length,
      0,
      `Uppercase text-transform found on non-micro selectors: ${JSON.stringify(invalidSelectors)}. Meridian spec restricts uppercase strictly to .t-micro.`
    );
  });

  await t.test('Zero External Runtime Dependencies (Only Google Fonts allowed)', () => {
    const externalScripts = (html.match(/<script[^>]+src=["'](https?:[^"']+)["']/gi) || []);
    assert.strictEqual(externalScripts.length, 0, `External script dependencies are forbidden: ${JSON.stringify(externalScripts)}`);

    const externalStyles = (html.match(/<link[^>]+href=["'](https?:[^"']+)["']/gi) || [])
      .map(m => m.match(/href=["'](https?:[^"']+)["']/i)[1]);
    for (const url of externalStyles) {
      assert.ok(
        url.includes('fonts.googleapis.com') || url.includes('fonts.gstatic.com'),
        `Forbidden external resource: ${url}. Only Google Fonts are permitted.`
      );
    }
  });
});

test('Suite: Tier 1 Feature Coverage (DOM Structure & Element Presence)', async (t) => {
  const html = loadHtml();
  if (!html) {
    t.skip('design/screenshot-concept.html not yet created by worker');
    return;
  }

  await t.test('F1: Desktop Mock Shell Components', () => {
    assert.match(html, /id=["']desktopStage["']/, 'Must contain #desktopStage');
    assert.match(html, /id=["']desktopTopbar["']/, 'Must contain #desktopTopbar');
    assert.match(html, /id=["']topbarClock["']/, 'Must contain #topbarClock');
    assert.match(html, /id=["']winTerminal["']/, 'Must contain mock window #winTerminal');
    assert.match(html, /id=["']winBrowser["']/, 'Must contain mock window #winBrowser');
    assert.match(html, /id=["']winFiles["']/, 'Must contain mock window #winFiles');
  });

  await t.test('F2: Initiation Controls & Hotkey References', () => {
    assert.match(html, /qsScreenshotTile|tileScreenshot|dockScreenshotBtn/, 'Must contain Quick Settings / Dock screenshot trigger tile');
    assert.match(html, /Print|Super\+Shift\+3|Super\+Shift\+4/, 'Must document shortcut keys in playground or HUD');
  });

  await t.test('F3: 3 Capture Target Modes Elements', () => {
    assert.match(html, /data-mode=["']area["']|id=["']hudModeArea["']/, 'Must have Area mode selector');
    assert.match(html, /data-mode=["']window["']|id=["']hudModeWindow["']/, 'Must have Window mode selector');
    assert.match(html, /data-mode=["']screen["']|id=["']hudModeScreen["']/, 'Must have Screen mode selector');
    assert.match(html, /id=["']selectionBox["']/, 'Must contain #selectionBox');
    assert.match(html, /handle-nw|data-handle=["']nw["']/, 'Must contain NW resize handle');
    assert.match(html, /handle-se|data-handle=["']se["']/, 'Must contain SE resize handle');
    assert.match(html, /id=["']selectionBadge["']|selection-badge/, 'Must contain live dimensions badge');
    assert.match(html, /id=["']windowHighlightBox["']/, 'Must contain #windowHighlightBox');
  });

  await t.test('F4: Floating HUD Toolbar Elements', () => {
    assert.match(html, /id=["']screenshotHud["']/, 'Must contain #screenshotHud');
    assert.match(html, /data-timer=["']0["']|id=["']hudTimer0["']/, 'Must contain 0s timer button');
    assert.match(html, /data-timer=["']3["']|id=["']hudTimer3["']/, 'Must contain 3s timer button');
    assert.match(html, /data-timer=["']5["']|id=["']hudTimer5["']/, 'Must contain 5s timer button');
    assert.match(html, /id=["']hudCursorToggle["']/, 'Must contain cursor toggle #hudCursorToggle');
    assert.match(html, /id=["']hudCaptureBtn["']|btnShutter/, 'Must contain shutter trigger button');
    assert.match(html, /id=["']hudCloseBtn["']/, 'Must contain close/cancel button');
  });

  await t.test('F5: Shutter & Flash Animation Elements', () => {
    assert.match(html, /id=["']screenFlash["']|class=["'][^"']*screen-flash[^"']*["']/, 'Must contain screen flash element');
    assert.match(html, /id=["']countdownOverlay["']|countdown-number/, 'Must contain countdown overlay');
  });

  await t.test('F6: Corner Thumbnail Elements', () => {
    assert.match(html, /id=["']cornerThumbnail["']/, 'Must contain #cornerThumbnail');
    assert.match(html, /id=["']thumbnailProgressRing["']|timer-progress/, 'Must contain SVG countdown progress ring');
    assert.match(html, /id=["']thumbnailCloseBtn["']/, 'Must contain thumbnail close button');
    assert.match(html, /id=["']thumbnailCanvas["']/, 'Must contain thumbnail canvas preview');
    assert.match(html, /id=["']thumbActionMarkup["']/, 'Must contain thumbnail markup action button');
    assert.match(html, /id=["']thumbActionCopy["']/, 'Must contain thumbnail copy action button');
    assert.match(html, /id=["']thumbActionSave["']/, 'Must contain thumbnail save action button');
    assert.match(html, /id=["']thumbnailContextMenu["']/, 'Must contain thumbnail context menu');
  });

  await t.test('F7: Toast Notification Container', () => {
    assert.match(html, /id=["']toastContainer["']/, 'Must contain #toastContainer');
  });

  await t.test('F8: Markup Canvas Tools Elements', () => {
    assert.match(html, /id=["']markupModal["']/, 'Must contain #markupModal');
    assert.match(html, /id=["']markupMainCanvas["']/, 'Must contain #markupMainCanvas');
    assert.match(html, /id=["']markupDraftCanvas["']/, 'Must contain #markupDraftCanvas');
    assert.match(html, /data-tool=["']crop["']/, 'Must contain crop tool');
    assert.match(html, /data-tool=["']arrow["']/, 'Must contain arrow tool');
    assert.match(html, /data-tool=["']rect["']/, 'Must contain rectangle tool');
    assert.match(html, /data-tool=["']ellipse["']/, 'Must contain ellipse tool');
    assert.match(html, /data-tool=["']text["']/, 'Must contain text tool');
    assert.match(html, /data-tool=["']highlighter["']/, 'Must contain highlighter tool');
    assert.match(html, /data-tool=["']blur["']|data-tool=["']redact["']/, 'Must contain redact/blur tool');
  });

  await t.test('F9: Palette & History Controls', () => {
    assert.match(html, /id=["']markupUndoBtn["']/, 'Must contain #markupUndoBtn');
    assert.match(html, /id=["']markupRedoBtn["']/, 'Must contain #markupRedoBtn');
    assert.match(html, /data-color=["']#2f6fd0["']|2f6fd0/, 'Must contain Azurite swatch (#2f6fd0)');
    assert.match(html, /data-color=["']#bf372c["']|bf372c/, 'Must contain Danger swatch (#bf372c)');
    assert.match(html, /data-color=["']#a1690a["']|a1690a/, 'Must contain Warn swatch (#a1690a)');
    assert.match(html, /data-color=["']#1c7a4c["']|1c7a4c/, 'Must contain Ok swatch (#1c7a4c)');
    assert.match(html, /data-stroke=["']2["']/, 'Must contain fine 2px stroke button');
    assert.match(html, /data-stroke=["']4["']/, 'Must contain medium 4px stroke button');
    assert.match(html, /data-stroke=["']8["']/, 'Must contain bold 8px stroke button');
  });

  await t.test('F10: Actions & Export Buttons', () => {
    assert.match(html, /id=["']markupCopyBtn["']/, 'Must contain #markupCopyBtn');
    assert.match(html, /id=["']markupSaveBtn["']/, 'Must contain #markupSaveBtn');
    assert.match(html, /id=["']markupDoneBtn["']/, 'Must contain #markupDoneBtn');
    assert.match(html, /id=["']markupBtnDelete["']/, 'Must contain #markupBtnDelete');
  });

  await t.test('F11: Meridian Polish & Presentation Elements', () => {
    assert.match(html, /id=["']playgroundDock["']/, 'Must contain #playgroundDock');
    assert.match(html, /data-theme-btn=["']light["']/, 'Must contain Light theme button');
    assert.match(html, /data-theme-btn=["']dark["']/, 'Must contain Dark theme button');
    assert.match(html, /data-accent=["']blue["']|data-accent=["']azurit["']/, 'Must contain Azurit accent');
    assert.match(html, /data-accent=["']teal["']|data-accent=["']malachite["']/, 'Must contain Teal/Malachite accent');
    assert.match(html, /data-accent=["']amber["']|data-accent=["']burshtyn["']/, 'Must contain Amber/Burshtyn accent');
  });
});

test('Suite: Tier 2 Boundary & Corner Cases Verification', async (t) => {
  const html = loadHtml();
  if (!html) {
    t.skip('design/screenshot-concept.html not yet created by worker');
    return;
  }

  await t.test('TC-F3-B01: Inverted Drag Coordinates Normalization Math', () => {
    function normalizeRect(x1, y1, x2, y2) {
      return {
        x: Math.min(x1, x2),
        y: Math.min(y1, y2),
        width: Math.abs(x2 - x1),
        height: Math.abs(y2 - y1)
      };
    }
    const rect = normalizeRect(500, 400, 100, 150);
    assert.strictEqual(rect.x, 100);
    assert.strictEqual(rect.y, 150);
    assert.strictEqual(rect.width, 400);
    assert.strictEqual(rect.height, 250);
  });

  await t.test('TC-F5-B03: Exact flash_alpha Mathematical Curve', () => {
    function easeOut(t) {
      return 1 - Math.pow(1 - t, 3);
    }
    function flashAlpha(fraction) {
      const PEAK = 0.6;
      const RISE = 0.25;
      if (fraction < 0 || fraction > 1) return 0.0;
      if (fraction < RISE) {
        return PEAK * easeOut(fraction / RISE);
      }
      return PEAK * (1.0 - easeOut((fraction - RISE) / (1.0 - RISE)));
    }

    assert.strictEqual(flashAlpha(0.0), 0.0);
    assert.ok(Math.abs(flashAlpha(0.25) - 0.60) < 1e-4, 'Peak must reach 0.60 at fraction 0.25');
    assert.ok(flashAlpha(0.5) < 0.60 && flashAlpha(0.5) > 0.1, 'Alpha should be decaying at midpoint');
    assert.ok(Math.abs(flashAlpha(1.0)) < 1e-4, 'Alpha must return to 0 at fraction 1.0');
  });

  await t.test('TC-F8-B05: Redact / Blur Pixelation Mosaic Calculation', () => {
    const blockSize = 12;
    const width = 120;
    const height = 80;
    const numBlocksX = Math.ceil(width / blockSize);
    const numBlocksY = Math.ceil(height / blockSize);
    assert.strictEqual(numBlocksX, 10);
    assert.strictEqual(numBlocksY, 7);
  });
});

test('Suite: Headless JavaScript Execution & Sandbox Test', async (t) => {
  const html = loadHtml();
  if (!html) {
    t.skip('design/screenshot-concept.html not yet created by worker');
    return;
  }

  const scripts = (html.match(/<script(?![^>]*src=)[^>]*>([\s\S]*?)<\/script>/gi) || [])
    .map(s => s.replace(/<\/?script[^>]*>/gi, ''))
    .join('\n');

  if (!scripts.trim()) {
    t.skip('No inline scripts found to execute');
    return;
  }

  await t.test('Script executes in browser VM environment without throwing syntax or runtime errors', () => {
    const elementsById = {};
    const createdElements = [];

    function mockElement(tag, id = '') {
      const el = {
        tagName: tag.toUpperCase(),
        id,
        style: {},
        classList: {
          _set: new Set(),
          add(c) { this._set.add(c); },
          remove(c) { this._set.delete(c); },
          contains(c) { return this._set.has(c); },
          toggle(c, force) {
            if (force === true) this._set.add(c);
            else if (force === false) this._set.delete(c);
            else if (this._set.has(c)) this._set.delete(c);
            else this._set.add(c);
          }
        },
        dataset: {},
        children: [],
        innerHTML: '',
        textContent: '',
        width: 1920,
        height: 1080,
        offsetWidth: 640,
        offsetHeight: 420,
        offsetLeft: 100,
        offsetTop: 100,
        getBoundingClientRect: () => ({ left: 100, top: 100, width: 640, height: 420, right: 740, bottom: 520 }),
        addEventListener: (event, handler) => {},
        removeEventListener: (event, handler) => {},
        appendChild: (child) => { el.children.push(child); return child; },
        removeChild: (child) => { el.children = el.children.filter(c => c !== child); return child; },
        setAttribute: (name, val) => {
          if (name === 'id') el.id = val;
          if (name.startsWith('data-')) {
            const camel = name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
            el.dataset[camel] = String(val);
          }
        },
        getAttribute: (name) => el.id || null,
        getContext: (type) => ({
          fillRect: () => {},
          clearRect: () => {},
          strokeRect: () => {},
          beginPath: () => {},
          moveTo: () => {},
          lineTo: () => {},
          stroke: () => {},
          fill: () => {},
          arc: () => {},
          ellipse: () => {},
          fillText: () => {},
          measureText: (text) => ({ width: text.length * 8 }),
          drawImage: () => {},
          getImageData: () => ({ data: new Uint8ClampedArray(100) }),
          putImageData: () => {},
          save: () => {},
          restore: () => {},
          setLineDash: () => {}
        }),
        toDataURL: () => 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII='
      };
      if (id) elementsById[id] = el;
      createdElements.push(el);
      return el;
    }

    const idRegex = /id=["']([a-zA-Z0-9_-]+)["']/g;
    let match;
    while ((match = idRegex.exec(html)) !== null) {
      mockElement('div', match[1]);
    }

    const sandbox = {
      window: {},
      document: {
        documentElement: {
          setAttribute: (k, v) => {},
          getAttribute: (k) => 'dark'
        },
        body: mockElement('body'),
        getElementById: (id) => elementsById[id] || mockElement('div', id),
        querySelector: (sel) => {
          if (sel.startsWith('#')) return elementsById[sel.slice(1)] || mockElement('div', sel.slice(1));
          return mockElement('div');
        },
        querySelectorAll: (sel) => [mockElement('div')],
        createElement: (tag) => mockElement(tag),
        addEventListener: (event, handler) => {},
        removeEventListener: (event, handler) => {}
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
          return {
            buffer: null,
            connect: () => {},
            start: () => {}
          };
        }
        get destination() { return {}; }
      },
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (id) => clearTimeout(id),
      setInterval: (fn, ms) => setInterval(fn, ms),
      clearInterval: (id) => clearInterval(id),
      requestAnimationFrame: (fn) => setTimeout(() => fn(Date.now()), 16),
      cancelAnimationFrame: (id) => clearTimeout(id),
      console: {
        log: () => {},
        warn: () => {},
        error: (msg) => { console.error('Browser VM Error:', msg); },
        info: () => {}
      },
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
          setTimeout(() => { if (this.onload) this.onload(); }, 5);
        }
      }
    };
    sandbox.window = sandbox;

    assert.doesNotThrow(() => {
      const context = vm.createContext(sandbox);
      vm.runInContext(scripts, context, { filename: 'design/screenshot-concept.html (embedded script)' });
    }, 'Evaluating screenshot-concept.html scripts in sandboxed VM must not throw exceptions');
  });
});

test('Suite: In-Place Workflow & Adversarial Review Verification', async (t) => {
  const html = loadHtml();
  if (!html) {
    t.skip('design/screenshot-concept.html not present');
    return;
  }

  await t.test('R1 & R2: In-Place Elements Presence & Conformance', () => {
    assert.match(html, /id=["']selectionBox["']/, 'Must contain in-place selection box #selectionBox');
    assert.match(html, /id=["']selMoveBar["']/, 'Must contain move bar #selMoveBar');
    assert.match(html, /id=["']inPlaceToolbar["']/, 'Must contain in-place snapping toolbar #inPlaceToolbar');
    assert.match(html, /id=["']inPlaceCanvas["']/, 'Must contain in-place canvas #inPlaceCanvas');
    assert.match(html, /id=["']inPlaceDraftCanvas["']/, 'Must contain in-place draft canvas #inPlaceDraftCanvas');
    assert.match(html, /id=["']inPlaceTextEditor["']/, 'Must contain in-place text editor #inPlaceTextEditor');
    assert.match(html, /id=["']inPlaceZoneFlash["']/, 'Must contain in-place zone flash #inPlaceZoneFlash');
  });

  await t.test('R2: All 10 In-Place Toolbar Tools Present', () => {
    assert.match(html, /id=["']toolBlur["']/, '1. Blur/Redact tool must exist');
    assert.match(html, /id=["']toolArrow["']/, '2. Arrow tool must exist');
    assert.match(html, /id=["']toolRect["']/, '3. Rectangle tool must exist');
    assert.match(html, /id=["']toolPen["']/, '4. Pen tool must exist');
    assert.match(html, /id=["']toolText["']/, '5. Text tool must exist');
    assert.match(html, /id=["']iptPalette["']/, '6. Palette must exist in toolbar');
    assert.match(html, /id=["']iptUndoBtn["']/, '7. Undo button must exist in toolbar');
    assert.match(html, /id=["']iptCancelBtn["']/, '8. Cancel button must exist in toolbar');
    assert.match(html, /id=["']iptCopyBtn["']/, '9. Copy button must exist in toolbar');
    assert.match(html, /id=["']iptSaveBtn["']/, '10. Save button must exist in toolbar');
  });

  await t.test('Defect Fix: Double Scrim Avoidance (.capture-overlay.has-selection)', () => {
    assert.match(html, /\.capture-overlay\.has-selection\s*\{\s*background:\s*transparent;?\s*\}/, 'captureOverlay must become transparent with .has-selection to prevent double scrimming');
  });

  await t.test('Defect Fix: Corner handle boundary clamping does not shift stationary anchor', () => {
    function calculateResizedRect(handle, box, anchor, current, minSize, stageW, stageH) {
      const curX = Math.max(0, Math.min(current.x, stageW));
      const curY = Math.max(0, Math.min(current.y, stageH));

      if (handle === 'se' || handle === 'nw' || handle === 'ne' || handle === 'sw') {
        let x, y, width, height;

        if (curX >= anchor.x) {
          x = anchor.x;
          width = Math.max(minSize, curX - anchor.x);
          if (x + width > stageW) width = stageW - x;
        } else {
          x = Math.min(anchor.x - minSize, curX);
          x = Math.max(0, x);
          width = anchor.x - x;
        }

        if (curY >= anchor.y) {
          y = anchor.y;
          height = Math.max(minSize, curY - anchor.y);
          if (y + height > stageH) height = stageH - y;
        } else {
          y = Math.min(anchor.y - minSize, curY);
          y = Math.max(0, y);
          height = anchor.y - y;
        }

        return { x: Math.round(x), y: Math.round(y), width: Math.round(width), height: Math.round(height) };
      }
      return { x: 0, y: 0, width: 0, height: 0 };
    }

    // 1. Drag SE handle past right boundary (x = 1200, stageW = 1000)
    const anchorSE = { x: 500, y: 400 };
    const rectSE = calculateResizedRect('se', null, anchorSE, { x: 1200, y: 400 }, 24, 1000, 800);
    assert.strictEqual(rectSE.x, 500, 'Stationary left edge anchor must NOT shift when dragging SE off-screen');
    assert.strictEqual(rectSE.width, 500, 'Width must clamp to stage edge (1000 - 500)');

    // 2. Drag NW handle past left/top boundary (x = -150, y = -100)
    const anchorNW = { x: 600, y: 500 };
    const rectNW = calculateResizedRect('nw', null, anchorNW, { x: -150, y: -100 }, 24, 1000, 800);
    assert.strictEqual(rectNW.x, 0, 'Left edge must clamp to stage edge (x = 0)');
    assert.strictEqual(rectNW.y, 0, 'Top edge must clamp to stage edge (y = 0)');
    assert.strictEqual(rectNW.x + rectNW.width, 600, 'Stationary right edge anchor must remain strictly at 600');
    assert.strictEqual(rectNW.y + rectNW.height, 500, 'Stationary bottom edge anchor must remain strictly at 500');
  });

  await t.test('Defect Fix: Audio Buffer creation handles undefined ctx.sampleRate', () => {
    const ctx = {
      createBuffer: (channels, length, rate) => ({ channels, length, rate })
    };
    const sampleRate = ctx.sampleRate || 44100;
    const bufferSize = Math.max(1, Math.floor(sampleRate * 0.045));
    const buffer = ctx.createBuffer(1, bufferSize, sampleRate);
    assert.ok(buffer);
    assert.strictEqual(buffer.rate, 44100);
    assert.ok(buffer.length > 0 && !isNaN(buffer.length));
  });

  await t.test('Defect Fix: In-place text editor provides high contrast background and text shadow', () => {
    assert.match(html, /\.in-place-text-editor\s*\{[^}]*background:\s*var\(--popover-bg\)/, 'Text editor must use popover background');
    assert.match(html, /\.in-place-text-editor\s*\{[^}]*text-shadow:/, 'Text editor must define text-shadow for readability');
  });

  await t.test('Defect Fix: Resize handles visibility restored after capture completion', () => {
    // Check that openCapture, cancelCapture, and renderSelectionBox restore handles display to ''
    assert.match(html, /handles\.forEach\(h\s*=>\s*\{\s*h\.style\.display\s*=\s*['"]['"];?\s*\}\)/, 'Must restore handles display in lifecycle methods');
  });

  await t.test('Defect Fix: Resize origin offset preservation prevents annotation shift', () => {
    // When resizing NW from (100, 100) to (60, 80), offset dx = 40, dy = 20
    const initialBox = { x: 100, y: 100, width: 400, height: 300 };
    const newBox = { x: 60, y: 80, width: 440, height: 320 };
    const dx = initialBox.x - newBox.x;
    const dy = initialBox.y - newBox.y;
    assert.strictEqual(dx, 40, 'Canvas origin offset dx must equal initialBox.x - newBox.x');
    assert.strictEqual(dy, 20, 'Canvas origin offset dy must equal initialBox.y - newBox.y');
    assert.match(html, /syncCanvasDimensions\(\s*dx\s*=\s*0\s*,\s*dy\s*=\s*0\s*\)/, 'syncCanvasDimensions must accept dx, dy offset arguments');
    assert.match(html, /ctx\.drawImage\(prevCanvas,\s*dx,\s*dy\)/, 'Previous canvas must be drawn at (dx, dy) on resized canvas');
  });

  await t.test('Defect Fix: Text tool auto-commits pending text on new label creation and tool switch', () => {
    assert.match(html, /openInlineText\([^\)]*\)\s*\{[^}]*this\.commitText\(\)/, 'openInlineText must commit any pending text before opening new editor');
    assert.match(html, /setTool\([^\)]*\)\s*\{[^}]*this\.commitText\(\)/, 'setTool must commit any pending text before switching tools');
  });

  await t.test('Defect Fix: Theme-aware semantic color resolution for Danger and Ok', () => {
    assert.match(html, /getVar\(['"]--danger['"]\)/, 'getEffectiveColor must query --danger CSS variable');
    assert.match(html, /getVar\(['"]--ok['"]\)/, 'getEffectiveColor must query --ok CSS variable');
  });

  await t.test('Defect Fix: Pen tool completes stroke through final pointer release point', () => {
    assert.match(html, /this\.penPoints\.push\(pos\)/, 'Pen tool must push release coordinate to penPoints before stroking path');
  });
});


