import test from 'node:test';
import assert from 'node:assert/strict';
import cp from 'node:child_process';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '..');
const TARGET_HTML_PATH = path.join(REPO_ROOT, 'design', 'screenshot-concept.html');

class ChromeSession {
  constructor(port = 9555) {
    this.port = port;
    this.tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), `chrome-stress-${port}-`));
    this.chromeProc = null;
    this.ws = null;
    this.msgId = 0;
    this.pending = new Map();
  }

  async start() {
    this.chromeProc = cp.spawn('google-chrome-stable', [
      '--headless=new',
      `--remote-debugging-port=${this.port}`,
      '--no-sandbox',
      '--disable-gpu',
      '--disable-extensions',
      `--user-data-dir=${this.tmpDir}`,
      `file://${TARGET_HTML_PATH}`
    ]);

    // Wait for CDP endpoint to be ready
    let target = null;
    const startTime = Date.now();
    while (Date.now() - startTime < 8000) {
      try {
        const res = await fetch(`http://127.0.0.1:${this.port}/json`);
        const list = await res.json();
        target = list.find(t => t.url && t.url.includes('screenshot-concept.html'));
        if (target && target.webSocketDebuggerUrl) break;
      } catch (_) {}
      await new Promise(r => setTimeout(r, 150));
    }

    if (!target || !target.webSocketDebuggerUrl) {
      this.stop();
      throw new Error(`Failed to connect to Chrome CDP for screenshot-concept.html on port ${this.port}`);
    }

    this.ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      this.ws.onopen = resolve;
      this.ws.onerror = reject;
    });

    this.ws.onmessage = (e) => {
      const d = JSON.parse(e.data);
      if (d.id && this.pending.has(d.id)) {
        const { resolve, reject } = this.pending.get(d.id);
        this.pending.delete(d.id);
        if (d.error) reject(new Error(JSON.stringify(d.error)));
        else resolve(d.result);
      }
    };
  }

  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.msgId;
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async eval(expression) {
    const res = await this.send('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true
    });
    if (res.exceptionDetails) {
      const txt = res.exceptionDetails.exception?.description || res.exceptionDetails.text;
      throw new Error(`Eval exception: ${txt}`);
    }
    return res.result ? res.result.value : undefined;
  }

  stop() {
    if (this.ws) {
      try { this.ws.close(); } catch (_) {}
      this.ws = null;
    }
    if (this.chromeProc) {
      try { this.chromeProc.kill(); } catch (_) {}
      this.chromeProc = null;
    }
    try {
      fs.rmSync(this.tmpDir, { recursive: true, force: true });
    } catch (_) {}
  }
}

test('Suite: Environmental & Architecture Readiness', async (t) => {
  await t.test('Target HTML file exists and is readable', () => {
    assert.ok(fs.existsSync(TARGET_HTML_PATH), 'screenshot-concept.html must exist');
    const content = fs.readFileSync(TARGET_HTML_PATH, 'utf8');
    assert.ok(content.length > 1000, 'HTML file must contain valid markup');
  });

  await t.test('Headless Chrome launches and exposes window.markupEditor', async () => {
    const session = new ChromeSession(9501);
    try {
      await session.start();
      const editorType = await session.eval('typeof window.markupEditor');
      assert.strictEqual(editorType, 'object', 'window.markupEditor must be exposed as object');
      const tool = await session.eval('window.markupEditor.tool');
      assert.ok(tool, 'Default tool must be defined');
    } finally {
      session.stop();
    }
  });
});

test('Suite 1: Tool Coverage (Arrow, Rect, Ellipse, Text, Highlighter, Redact / Blur, Crop)', async (t) => {
  const session = new ChromeSession(9502);
  await session.start();

  t.after(() => session.stop());

  await t.test('TC-M1.2-01: Tool selection & segmented control state sync', async () => {
    const res = await session.eval(`
      (() => {
        const ed = window.markupEditor;
        const tools = ['arrow', 'rect', 'ellipse', 'text', 'highlighter', 'blur', 'crop'];
        const results = [];
        for (const t of tools) {
          const btn = document.querySelector('#markupToolSelector button[data-tool="' + t + '"]');
          if (btn) btn.click();
          results.push({
            tool: t,
            activeTool: ed.tool,
            btnActive: btn ? btn.classList.contains('active') : false,
            cropBtnDisplay: ed.applyCropBtn ? ed.applyCropBtn.style.display : null
          });
        }
        return results;
      })()
    `);

    for (const r of res) {
      assert.strictEqual(r.activeTool, r.tool, `Tool ${r.tool} must activate`);
      assert.strictEqual(r.btnActive, true, `Button for ${r.tool} must have active class`);
      if (r.tool === 'crop') {
        assert.strictEqual(r.cropBtnDisplay, 'inline-flex', 'Apply crop button must show for crop tool');
      } else {
        assert.strictEqual(r.cropBtnDisplay, 'none', `Apply crop button must be hidden for ${r.tool}`);
      }
    }
  });

  await t.test('TC-M1.2-02: Arrow tool stroke & arrowhead drawing', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 400; c.height = 300;
        const ctx = c.getContext('2d');
        ctx.fillStyle = '#ffffff';
        ctx.fillRect(0, 0, 400, 300);
        window.markupEditor.open(c.toDataURL(), { width: 400, height: 300 });

        setTimeout(() => {
          const ed = window.markupEditor;
          ed.setTool('arrow');
          ed.color = '#2f6fd0'; // Azurite
          ed.strokeWidth = 4;

          const draft = ed.draftCanvas;
          const rect = draft.getBoundingClientRect();
          const startX = rect.left + 50 * (rect.width / 400);
          const startY = rect.top + 50 * (rect.height / 300);
          const endX = rect.left + 200 * (rect.width / 400);
          const endY = rect.top + 150 * (rect.height / 300);

          draft.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, clientX: startX, clientY: startY }));
          window.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, cancelable: true, clientX: endX, clientY: endY }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, cancelable: true, clientX: endX, clientY: endY }));

          const mCtx = ed.mainCanvas.getContext('2d');
          const tipPixel = Array.from(mCtx.getImageData(200, 150, 1, 1).data);
          const untouched = Array.from(mCtx.getImageData(350, 250, 1, 1).data);

          resolve({
            historyIndex: ed.historyIndex,
            tipPixel,
            untouched
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.historyIndex, 1, 'Arrow drawing must increment history index to 1');
    assert.deepStrictEqual(res.tipPixel, [47, 111, 208, 255], 'Arrow tip must match Azurite (#2f6fd0)');
    assert.deepStrictEqual(res.untouched, [255, 255, 255, 255], 'Untouched canvas area must remain white');
  });

  await t.test('TC-M1.2-03: Rectangle tool outline vs hollow interior', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 400; c.height = 300;
        const ctx = c.getContext('2d');
        ctx.fillStyle = '#ffffff';
        ctx.fillRect(0, 0, 400, 300);
        window.markupEditor.open(c.toDataURL(), { width: 400, height: 300 });

        setTimeout(() => {
          const ed = window.markupEditor;
          ed.setTool('rect');
          ed.color = '#bf372c'; // Danger red
          ed.strokeWidth = 4;

          const draft = ed.draftCanvas;
          const rect = draft.getBoundingClientRect();
          const startX = rect.left + 80 * (rect.width / 400);
          const startY = rect.top + 60 * (rect.height / 300);
          const endX = rect.left + 240 * (rect.width / 400);
          const endY = rect.top + 180 * (rect.height / 300);

          draft.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, clientX: startX, clientY: startY }));
          window.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, cancelable: true, clientX: endX, clientY: endY }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, cancelable: true, clientX: endX, clientY: endY }));

          const mCtx = ed.mainCanvas.getContext('2d');
          const borderPixel = Array.from(mCtx.getImageData(80, 120, 1, 1).data);
          const interiorPixel = Array.from(mCtx.getImageData(160, 120, 1, 1).data);

          resolve({
            historyIndex: ed.historyIndex,
            borderPixel,
            interiorPixel
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.historyIndex, 1, 'Rect drawing must increment history index to 1');
    assert.deepStrictEqual(res.borderPixel, [191, 55, 44, 255], 'Rect border must match Danger red (#bf372c)');
    assert.deepStrictEqual(res.interiorPixel, [255, 255, 255, 255], 'Rect interior must remain hollow/white');
  });

  await t.test('TC-M1.2-04: Ellipse tool perimeter vs hollow interior', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 400; c.height = 300;
        const ctx = c.getContext('2d');
        ctx.fillStyle = '#ffffff';
        ctx.fillRect(0, 0, 400, 300);
        window.markupEditor.open(c.toDataURL(), { width: 400, height: 300 });

        setTimeout(() => {
          const ed = window.markupEditor;
          ed.setTool('ellipse');
          ed.color = '#1c7a4c'; // Ok green
          ed.strokeWidth = 4;

          const draft = ed.draftCanvas;
          const rect = draft.getBoundingClientRect();
          const startX = rect.left + 100 * (rect.width / 400);
          const startY = rect.top + 80 * (rect.height / 300);
          const endX = rect.left + 300 * (rect.width / 400);
          const endY = rect.top + 200 * (rect.height / 300);

          draft.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, clientX: startX, clientY: startY }));
          window.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, cancelable: true, clientX: endX, clientY: endY }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, cancelable: true, clientX: endX, clientY: endY }));

          const mCtx = ed.mainCanvas.getContext('2d');
          const topPerimeter = Array.from(mCtx.getImageData(200, 80, 1, 1).data);
          const centerPixel = Array.from(mCtx.getImageData(200, 140, 1, 1).data);

          resolve({
            historyIndex: ed.historyIndex,
            topPerimeter,
            centerPixel
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.historyIndex, 1, 'Ellipse drawing must increment history index to 1');
    assert.deepStrictEqual(res.topPerimeter, [28, 122, 76, 255], 'Ellipse top perimeter must match Ok green (#1c7a4c)');
    assert.deepStrictEqual(res.centerPixel, [255, 255, 255, 255], 'Ellipse center must remain hollow/white');
  });

  await t.test('TC-M1.2-05: Inline Text tool placement, editing & commit', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 400; c.height = 300;
        const ctx = c.getContext('2d');
        ctx.fillStyle = '#ffffff';
        ctx.fillRect(0, 0, 400, 300);
        window.markupEditor.open(c.toDataURL(), { width: 400, height: 300 });

        setTimeout(() => {
          const ed = window.markupEditor;
          ed.setTool('text');
          ed.color = '#a1690a'; // Warn amber

          const draft = ed.draftCanvas;
          const rect = draft.getBoundingClientRect();
          const startX = rect.left + 60 * (rect.width / 400);
          const startY = rect.top + 80 * (rect.height / 300);

          draft.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, clientX: startX, clientY: startY }));

          const editor = ed.textEditorEl;
          const wasOpen = editor.style.display !== 'none';
          editor.textContent = 'Meridian Security Notice';

          // Commit text
          editor.dispatchEvent(new Event('blur'));
          const wasClosed = editor.style.display === 'none';

          const mCtx = ed.mainCanvas.getContext('2d');
          const textBlock = mCtx.getImageData(60, 80, 220, 30);
          let nonWhite = 0;
          for (let i = 0; i < textBlock.data.length; i += 4) {
            if (textBlock.data[i] !== 255 || textBlock.data[i+1] !== 255 || textBlock.data[i+2] !== 255) {
              nonWhite++;
            }
          }

          resolve({
            wasOpen,
            wasClosed,
            historyIndex: ed.historyIndex,
            nonWhite
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.wasOpen, true, 'Inline text editor must reveal on pointer down');
    assert.strictEqual(res.wasClosed, true, 'Inline text editor must hide on commit/blur');
    assert.strictEqual(res.historyIndex, 1, 'Text commit must save history state');
    assert.ok(res.nonWhite > 500, `Text must render readable raster glyphs (found ${res.nonWhite} non-white pixels)`);
  });

  await t.test('TC-M1.2-06: Highlighter multiply blend translucent overlay', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 400; c.height = 300;
        const ctx = c.getContext('2d');
        // Gray background
        ctx.fillStyle = '#808080';
        ctx.fillRect(0, 0, 400, 300);
        window.markupEditor.open(c.toDataURL(), { width: 400, height: 300 });

        setTimeout(() => {
          const ed = window.markupEditor;
          ed.setTool('highlighter');
          ed.color = '#e5b355'; // Yellow highlighter

          const draft = ed.draftCanvas;
          const rect = draft.getBoundingClientRect();
          const startX = rect.left + 50 * (rect.width / 400);
          const startY = rect.top + 150 * (rect.height / 300);
          const endX = rect.left + 250 * (rect.width / 400);
          const endY = rect.top + 150 * (rect.height / 300);

          draft.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, clientX: startX, clientY: startY }));
          window.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, cancelable: true, clientX: endX, clientY: endY }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, cancelable: true, clientX: endX, clientY: endY }));

          const mCtx = ed.mainCanvas.getContext('2d');
          const blendedPixel = Array.from(mCtx.getImageData(150, 150, 1, 1).data);

          resolve({
            historyIndex: ed.historyIndex,
            blendedPixel
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.historyIndex, 1, 'Highlighter must save history state');
    assert.ok(res.blendedPixel[0] < 128, 'Red channel must darken under multiply blend');
    assert.ok(res.blendedPixel[1] < 128, 'Green channel must darken under multiply blend');
    assert.ok(res.blendedPixel[2] < 128, 'Blue channel must darken under multiply blend');
  });
});

test('Suite 2: Redact / Blur Privacy & Obscuration Verification', async (t) => {
  const session = new ChromeSession(9503);
  await session.start();

  t.after(() => session.stop());

  await t.test('TC-M1.2-07: Spatial entropy obliteration: 100% intra-block flatting on alternating stripes', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 120; c.height = 80;
        const ctx = c.getContext('2d');
        // Alternating 1px black (0) and white (255) stripes to simulate confidential text/barcode
        for (let x = 0; x < 120; x++) {
          ctx.fillStyle = (x % 2 === 0) ? '#000000' : '#ffffff';
          ctx.fillRect(x, 0, 1, 80);
        }
        window.markupEditor.open(c.toDataURL(), { width: 120, height: 80 });

        setTimeout(() => {
          const ed = window.markupEditor;
          const mCanvas = ed.mainCanvas;
          const mCtx = mCanvas.getContext('2d');

          // Apply Redact/Blur over entire canvas
          ed.applyPixelateMosaic(mCtx, 0, 0, 120, 80);

          let totalBlocks = 0;
          let nonUniformBlocks = 0;
          let blockColors = [];

          // Inspect every 10x10 block
          for (let by = 0; by < 80; by += 10) {
            for (let bx = 0; bx < 120; bx += 10) {
              totalBlocks++;
              const blkData = mCtx.getImageData(bx, by, 10, 10).data;
              const r0 = blkData[0], g0 = blkData[1], b0 = blkData[2];
              let variance = 0;
              for (let p = 0; p < blkData.length; p += 4) {
                variance += Math.abs(blkData[p] - r0) + Math.abs(blkData[p+1] - g0) + Math.abs(blkData[p+2] - b0);
              }
              if (variance > 0) nonUniformBlocks++;
              blockColors.push(r0);
            }
          }

          resolve({
            totalBlocks,
            nonUniformBlocks,
            meanBlockColor: blockColors.reduce((a, b) => a + b, 0) / blockColors.length
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.totalBlocks, 96, 'Must inspect exactly 96 10x10 blocks in 120x80 canvas');
    assert.strictEqual(res.nonUniformBlocks, 0, 'Every single 10x10 block must have ZERO intra-block variance');
    assert.strictEqual(Math.round(res.meanBlockColor), 128, 'Average block color must be arithmetic mean of 0 and 255 (~128)');
  });

  await t.test('TC-M1.2-08: Sub-block dimensions resilience (5x5, 1x1, 0x0, negative)', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 100; c.height = 100;
        const ctx = c.getContext('2d');
        ctx.fillStyle = '#ff0000';
        ctx.fillRect(0, 0, 100, 100);
        window.markupEditor.open(c.toDataURL(), { width: 100, height: 100 });

        setTimeout(() => {
          const ed = window.markupEditor;
          const mCtx = ed.mainCanvas.getContext('2d');

          // Sub-block: 5x5 region
          ed.applyPixelateMosaic(mCtx, 10, 10, 5, 5);
          const subData = mCtx.getImageData(10, 10, 5, 5).data;
          const subR = subData[0];

          // 1x1 region
          ed.applyPixelateMosaic(mCtx, 20, 20, 1, 1);
          const oneData = mCtx.getImageData(20, 20, 1, 1).data;

          // 0x0 region (should no-op without error)
          ed.applyPixelateMosaic(mCtx, 30, 30, 0, 0);

          // Negative dimensions (should no-op without error)
          ed.applyPixelateMosaic(mCtx, 30, 30, -10, -10);

          resolve({
            subR,
            oneR: oneData[0]
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.subR, 255, '5x5 sub-block must process successfully');
    assert.strictEqual(res.oneR, 255, '1x1 sub-block must process successfully');
  });

  await t.test('TC-M1.2-09: Overlapping redact strokes idempotency', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 100; c.height = 100;
        const ctx = c.getContext('2d');
        ctx.fillStyle = '#4488cc';
        ctx.fillRect(0, 0, 100, 100);
        window.markupEditor.open(c.toDataURL(), { width: 100, height: 100 });

        setTimeout(() => {
          const ed = window.markupEditor;
          const mCtx = ed.mainCanvas.getContext('2d');

          // Pass 1
          ed.applyPixelateMosaic(mCtx, 10, 10, 40, 40);
          const pass1Data = Array.from(mCtx.getImageData(10, 10, 40, 40).data);

          // Pass 2 over exact same region
          ed.applyPixelateMosaic(mCtx, 10, 10, 40, 40);
          const pass2Data = Array.from(mCtx.getImageData(10, 10, 40, 40).data);

          let diffCount = 0;
          for (let i = 0; i < pass1Data.length; i++) {
            if (pass1Data[i] !== pass2Data[i]) diffCount++;
          }

          resolve({ diffCount });
        }, 80);
      })
    `);

    assert.strictEqual(res.diffCount, 0, 'Applying redact over already redacted blocks must be completely idempotent');
  });
});

test('Suite 3: Undo / Redo Stack Limits & Clamping Stress', async (t) => {
  const session = new ChromeSession(9504);
  await session.start();

  t.after(() => session.stop());

  await t.test('TC-M1.2-10: Pushing 35 edits against 30 limit clamps stack cleanly with no leak', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 100; c.height = 100;
        window.markupEditor.open(c.toDataURL(), { width: 100, height: 100 });

        setTimeout(() => {
          const ed = window.markupEditor;
          const ctx = ed.mainCanvas.getContext('2d');

          // Push 35 consecutive distinct edits
          for (let i = 1; i <= 35; i++) {
            ctx.fillStyle = 'rgb(' + (i * 7) + ', 0, 0)';
            ctx.fillRect(0, 0, 10, 10);
            ed.saveHistoryState();
          }

          resolve({
            stackLength: ed.historyStack.length,
            historyIndex: ed.historyIndex,
            undoDisabled: ed.undoBtn.disabled,
            redoDisabled: ed.redoBtn.disabled
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.stackLength, 30, 'History stack must clamp strictly to max 30 states');
    assert.strictEqual(res.historyIndex, 29, 'History index must be clamped to 29 (30 - 1)');
    assert.strictEqual(res.undoDisabled, false, 'Undo button must be enabled when at head of history');
    assert.strictEqual(res.redoDisabled, true, 'Redo button must be disabled when at head of history');
  });

  await t.test('TC-M1.2-11: Undo underflow stress (undo past zero)', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const ed = window.markupEditor;
        // Undo 40 times (exceeding 30 stack length)
        for (let i = 0; i < 40; i++) {
          ed.undo();
        }

        resolve({
          historyIndex: ed.historyIndex,
          undoDisabled: ed.undoBtn.disabled,
          redoDisabled: ed.redoBtn.disabled
        });
      })
    `);

    assert.strictEqual(res.historyIndex, 0, 'History index must stop at 0 and never underflow to negative');
    assert.strictEqual(res.undoDisabled, true, 'Undo button must disable at index 0');
    assert.strictEqual(res.redoDisabled, false, 'Redo button must remain enabled at index 0');
  });

  await t.test('TC-M1.2-12: Redo overflow stress (redo past end)', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const ed = window.markupEditor;
        // Redo 40 times
        for (let i = 0; i < 40; i++) {
          ed.redo();
        }

        resolve({
          historyIndex: ed.historyIndex,
          undoDisabled: ed.undoBtn.disabled,
          redoDisabled: ed.redoBtn.disabled
        });
      })
    `);

    assert.strictEqual(res.historyIndex, 29, 'History index must stop at stack.length - 1 (29)');
    assert.strictEqual(res.redoDisabled, true, 'Redo button must disable at top of history');
    assert.strictEqual(res.undoDisabled, false, 'Undo button must remain enabled at top of history');
  });

  await t.test('TC-M1.2-13: History branch truncation after partial undo', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const ed = window.markupEditor;
        // Undo 10 times to index 19
        for (let i = 0; i < 10; i++) ed.undo();
        const indexBeforeBranch = ed.historyIndex;

        // Perform 1 new edit
        const ctx = ed.mainCanvas.getContext('2d');
        ctx.fillStyle = '#00ff00';
        ctx.fillRect(50, 50, 20, 20);
        ed.saveHistoryState();

        resolve({
          indexBeforeBranch,
          newStackLength: ed.historyStack.length,
          newIndex: ed.historyIndex,
          redoDisabled: ed.redoBtn.disabled
        });
      })
    `);

    assert.strictEqual(res.indexBeforeBranch, 19, 'Must undo to state 19');
    assert.strictEqual(res.newStackLength, 21, 'Stack length must become 20 prior states + 1 new = 21');
    assert.strictEqual(res.newIndex, 20, 'Head index must point to state 20');
    assert.strictEqual(res.redoDisabled, true, 'Redo button must disable after branching new action');
  });
});

test('Suite 4: Crop Boundary Cases & Canvas Geometry', async (t) => {
  const session = new ChromeSession(9505);
  await session.start();

  t.after(() => session.stop());

  await t.test('TC-M1.2-14: Sub-minimum crop rejection (<10px)', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 400; c.height = 300;
        window.markupEditor.open(c.toDataURL(), { width: 400, height: 300 });

        setTimeout(() => {
          const ed = window.markupEditor;
          // Attempt sub-minimum crop (8x8)
          ed.currentCropBox = { x: 50, y: 50, width: 8, height: 8 };
          ed.commitCrop();

          resolve({
            w: ed.mainCanvas.width,
            h: ed.mainCanvas.height
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.w, 400, 'Sub-minimum crop width must be ignored');
    assert.strictEqual(res.h, 300, 'Sub-minimum crop height must be ignored');
  });

  await t.test('TC-M1.2-15: Minimum permitted crop threshold (exactly 10x10)', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const ed = window.markupEditor;
        ed.currentCropBox = { x: 50, y: 50, width: 10, height: 10 };
        ed.commitCrop();

        resolve({
          w: ed.mainCanvas.width,
          h: ed.mainCanvas.height,
          draftW: ed.draftCanvas.width,
          draftH: ed.draftCanvas.height,
          badge: ed.dimsBadge.textContent
        });
      })
    `);

    assert.strictEqual(res.w, 10, 'Minimum 10px crop width must be accepted');
    assert.strictEqual(res.h, 10, 'Minimum 10px crop height must be accepted');
    assert.strictEqual(res.draftW, 10, 'Draft canvas width must sync to 10px');
    assert.strictEqual(res.draftH, 10, 'Draft canvas height must sync to 10px');
    assert.strictEqual(res.badge, '10 × 10 px', 'Badge must display 10 × 10 px');
  });

  await t.test('TC-M1.2-16: Inverted drag crop coordinate normalization & commit', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 500; c.height = 400;
        window.markupEditor.open(c.toDataURL(), { width: 500, height: 400 });

        setTimeout(() => {
          const ed = window.markupEditor;
          ed.setTool('crop');

          // Simulate inverted drag: start at (350, 300), end at (100, 80)
          ed.startPos = { x: 350, y: 300 };
          const pos = { x: 100, y: 80 };
          ed.currentCropBox = {
            x: Math.min(ed.startPos.x, pos.x),
            y: Math.min(ed.startPos.y, pos.y),
            width: Math.abs(pos.x - ed.startPos.x),
            height: Math.abs(pos.y - ed.startPos.y)
          };

          const normalized = { ...ed.currentCropBox };
          ed.commitCrop();

          resolve({
            normalized,
            w: ed.mainCanvas.width,
            h: ed.mainCanvas.height,
            badge: ed.dimsBadge.textContent,
            toolAfterCrop: ed.tool
          });
        }, 80);
      })
    `);

    assert.deepStrictEqual(res.normalized, { x: 100, y: 80, width: 250, height: 220 }, 'Inverted drag coordinates must normalize correctly');
    assert.strictEqual(res.w, 250, 'Canvas width must resize to crop width (250)');
    assert.strictEqual(res.h, 220, 'Canvas height must resize to crop height (220)');
    assert.strictEqual(res.badge, '250 × 220 px', 'Badge text must reflect 250 × 220 px');
    assert.strictEqual(res.toolAfterCrop, 'arrow', 'Tool must reset to arrow following crop commit');
  });

  await t.test('TC-M1.2-17: Multiple sequential crops progressive resizing', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const ed = window.markupEditor;
        // Crop 1: down to 180x150
        ed.currentCropBox = { x: 10, y: 10, width: 180, height: 150 };
        ed.commitCrop();
        const w1 = ed.mainCanvas.width;
        const h1 = ed.mainCanvas.height;

        // Crop 2: down to 90x80
        ed.currentCropBox = { x: 5, y: 5, width: 90, height: 80 };
        ed.commitCrop();
        const w2 = ed.mainCanvas.width;
        const h2 = ed.mainCanvas.height;

        resolve({ w1, h1, w2, h2 });
      })
    `);

    assert.strictEqual(res.w1, 180, 'First crop width must be 180');
    assert.strictEqual(res.h1, 150, 'First crop height must be 150');
    assert.strictEqual(res.w2, 90, 'Second crop width must be 90');
    assert.strictEqual(res.h2, 80, 'Second crop height must be 80');
  });

  await t.test('TC-M1.2-18: Adversarial finding: Undo after crop behavior audit', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const ed = window.markupEditor;
        const wBeforeUndo = ed.mainCanvas.width;
        const hBeforeUndo = ed.mainCanvas.height;
        const prevStackEntry = ed.historyStack[ed.historyIndex - 1];

        ed.undo();
        const wAfterUndo = ed.mainCanvas.width;
        const hAfterUndo = ed.mainCanvas.height;
        const badgeAfterUndo = ed.dimsBadge.textContent;

        resolve({
          wBeforeUndo,
          hBeforeUndo,
          priorEntryWidth: prevStackEntry ? prevStackEntry.width : null,
          priorEntryHeight: prevStackEntry ? prevStackEntry.height : null,
          wAfterUndo,
          hAfterUndo,
          badgeAfterUndo,
          canvasResizedBack: wAfterUndo === (prevStackEntry ? prevStackEntry.width : null)
        });
      })
    `);

    // In HTML Canvas, putImageData does not resize canvas.width/height unless explicitly set.
    // Documenting whether implementation resizes canvas back or leaves it cropped.
    assert.strictEqual(res.canvasResizedBack, false, 'Adversarial observation: undo() restores pixel buffer via putImageData but does not re-assign canvas.width/height or update dimsBadge');
  });
});

test('Suite 5: Clipboard Fallback & Error Resilience', async (t) => {
  const session = new ChromeSession(9506);
  await session.start();

  t.after(() => session.stop());

  await t.test('TC-M1.2-19: Simulated rejection of navigator.clipboard.write catches cleanly and triggers toast', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        let unhandledRejectionOccurred = false;
        const handler = () => { unhandledRejectionOccurred = true; };
        window.addEventListener('unhandledrejection', handler);

        const ed = window.markupEditor;
        // Mock rejection with NotAllowedError
        let callCount = 0;
        navigator.clipboard.write = () => {
          callCount++;
          return Promise.reject(new DOMException('The clipboard operation was denied.', 'NotAllowedError'));
        };

        const toastContainer = document.getElementById('toastContainer');
        toastContainer.innerHTML = '';

        ed.copy();

        setTimeout(() => {
          window.removeEventListener('unhandledrejection', handler);
          const toasts = toastContainer.querySelectorAll('.glide-toast');
          const lastToast = toasts[toasts.length - 1];
          const toastTitle = lastToast ? lastToast.querySelector('.toast-title').textContent : null;

          resolve({
            callCount,
            unhandledRejectionOccurred,
            toastShown: toasts.length > 0,
            toastTitle
          });
        }, 120);
      })
    `);

    assert.strictEqual(res.callCount, 1, 'navigator.clipboard.write must be invoked');
    assert.strictEqual(res.unhandledRejectionOccurred, false, 'Rejected clipboard promise must be caught cleanly without unhandled rejection');
    assert.strictEqual(res.toastShown, true, 'Toast notification must be displayed even when clipboard write rejects');
    assert.strictEqual(res.toastTitle, 'Знімок скопійовано в буфер', 'Toast title must match clipboard confirmation');
  });

  await t.test('TC-M1.2-20: Missing or null navigator.clipboard handles gracefully', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const ed = window.markupEditor;
        const toastContainer = document.getElementById('toastContainer');
        toastContainer.innerHTML = '';

        // Temporarily redefine clipboard via Prototype descriptor
        const origDesc = Object.getOwnPropertyDescriptor(Navigator.prototype, 'clipboard');
        Object.defineProperty(Navigator.prototype, 'clipboard', { get: () => undefined, configurable: true });

        let threw = false;
        try {
          ed.copy();
        } catch (_) {
          threw = true;
        }

        if (origDesc) {
          Object.defineProperty(Navigator.prototype, 'clipboard', origDesc);
        }

        setTimeout(() => {
          const toasts = toastContainer.querySelectorAll('.glide-toast');
          resolve({
            threw,
            toastShown: toasts.length > 0
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.threw, false, 'Absence of navigator.clipboard must not throw');
    assert.strictEqual(res.toastShown, true, 'Toast notification must still trigger as UI feedback');
  });

  await t.test('TC-M1.2-21: CornerThumbnailManager copyToClipboard error handling', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        let unhandledRejectionOccurred = false;
        const handler = () => { unhandledRejectionOccurred = true; };
        window.addEventListener('unhandledrejection', handler);

        const thumb = window.thumbnailManager;
        navigator.clipboard.write = () => Promise.reject(new DOMException('Access denied', 'NotAllowedError'));

        const toastContainer = document.getElementById('toastContainer');
        toastContainer.innerHTML = '';

        thumb.copyToClipboard();

        setTimeout(() => {
          window.removeEventListener('unhandledrejection', handler);
          const toasts = toastContainer.querySelectorAll('.glide-toast');
          resolve({
            unhandledRejectionOccurred,
            toastShown: toasts.length > 0
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.unhandledRejectionOccurred, false, 'Thumbnail copy rejection must be caught cleanly');
    assert.strictEqual(res.toastShown, true, 'Toast notification must be shown');
  });
});

test('Suite 6: Export & Discard Lifecycle Workflows', async (t) => {
  const session = new ChromeSession(9507);
  await session.start();

  t.after(() => session.stop());

  await t.test('TC-M1.2-22: Save PNG triggers download anchor with valid filename format', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const ed = window.markupEditor;
        const toastContainer = document.getElementById('toastContainer');
        toastContainer.innerHTML = '';

        let clickedDownload = null;
        const origClick = HTMLAnchorElement.prototype.click;
        HTMLAnchorElement.prototype.click = function() {
          clickedDownload = this.download;
        };

        ed.save();
        HTMLAnchorElement.prototype.click = origClick;

        setTimeout(() => {
          const toasts = toastContainer.querySelectorAll('.glide-toast');
          const lastToast = toasts[toasts.length - 1];
          const toastTitle = lastToast ? lastToast.querySelector('.toast-title').textContent : null;

          resolve({
            clickedDownload,
            toastTitle
          });
        }, 80);
      })
    `);

    assert.match(res.clickedDownload, /^Знімок_розмітка_\d{4}-\d{2}-\d{2}\.png$/, 'Filename must match Знімок_розмітка_YYYY-MM-DD.png');
    assert.strictEqual(res.toastTitle, 'Знімок збережено', 'Save toast notification must appear');
  });

  await t.test('TC-M1.2-23: Clear drawings resets canvas to initial clean state', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const c = document.createElement('canvas');
        c.width = 100; c.height = 100;
        const ctx = c.getContext('2d');
        ctx.fillStyle = '#ffffff';
        ctx.fillRect(0, 0, 100, 100);
        window.markupEditor.open(c.toDataURL(), { width: 100, height: 100 });

        setTimeout(() => {
          const ed = window.markupEditor;
          const mCtx = ed.mainCanvas.getContext('2d');

          // Draw red rectangle
          mCtx.fillStyle = '#ff0000';
          mCtx.fillRect(10, 10, 50, 50);
          ed.saveHistoryState();
          const idxBeforeClear = ed.historyIndex;

          // Clear drawings
          ed.clearDrawings();
          const idxAfterClear = ed.historyIndex;
          const pixelAfterClear = Array.from(mCtx.getImageData(25, 25, 1, 1).data);

          resolve({
            idxBeforeClear,
            idxAfterClear,
            pixelAfterClear
          });
        }, 80);
      })
    `);

    assert.strictEqual(res.idxBeforeClear, 1, 'Index before clear must be 1');
    assert.strictEqual(res.idxAfterClear, 2, 'Clear action must save new history state');
    assert.deepStrictEqual(res.pixelAfterClear, [255, 255, 255, 255], 'Canvas must be restored to initial clean white state');
  });

  await t.test('TC-M1.2-24: Done action commits save and closes modal', async () => {
    const res = await session.eval(`
      new Promise((resolve) => {
        const ed = window.markupEditor;
        ed.modalEl.style.display = 'flex';

        let saved = false;
        const origSave = ed.save;
        ed.save = () => { saved = true; };

        ed.done();
        ed.save = origSave;

        resolve({
          saved,
          modalDisplay: ed.modalEl.style.display
        });
      })
    `);

    assert.strictEqual(res.saved, true, 'Done must invoke save()');
    assert.strictEqual(res.modalDisplay, 'none', 'Done must close modal');
  });
});
