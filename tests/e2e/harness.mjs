import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';

export const CONCEPT_HTML_PATH = path.resolve(process.cwd(), 'design/dynamic-island-concept.html');

/**
 * Lightweight in-memory DOM Node and Element implementation
 */
export class DOMNode {
  constructor(nodeType, nodeName) {
    this.nodeType = nodeType; // 1: Element, 3: Text, 9: Document
    this.nodeName = nodeName;
    this.parentNode = null;
    this.childNodes = [];
  }

  get children() {
    return this.childNodes.filter(n => n.nodeType === 1);
  }

  appendChild(child) {
    if (child.parentNode) {
      child.parentNode.removeChild(child);
    }
    child.parentNode = this;
    this.childNodes.push(child);
    return child;
  }

  removeChild(child) {
    const idx = this.childNodes.indexOf(child);
    if (idx !== -1) {
      this.childNodes.splice(idx, 1);
      child.parentNode = null;
      return child;
    }
    return null;
  }

  replaceChild(newChild, oldChild) {
    const idx = this.childNodes.indexOf(oldChild);
    if (idx !== -1) {
      if (newChild.parentNode) newChild.parentNode.removeChild(newChild);
      this.childNodes[idx] = newChild;
      newChild.parentNode = this;
      oldChild.parentNode = null;
      return oldChild;
    }
    return null;
  }
}

class ClassList {
  constructor(element) {
    this._el = element;
  }

  _getClasses() {
    const cls = this._el.getAttribute('class') || '';
    return cls.trim().split(/\s+/).filter(Boolean);
  }

  _setClasses(arr) {
    this._el.setAttribute('class', arr.join(' '));
  }

  contains(cls) {
    return this._getClasses().includes(cls);
  }

  add(...classes) {
    const current = this._getClasses();
    for (const c of classes) {
      if (c && !current.includes(c)) current.push(c);
    }
    this._setClasses(current);
  }

  remove(...classes) {
    const current = this._getClasses().filter(c => !classes.includes(c));
    this._setClasses(current);
  }

  toggle(cls, force) {
    const exists = this.contains(cls);
    const shouldHave = force !== undefined ? force : !exists;
    if (shouldHave) {
      this.add(cls);
    } else {
      this.remove(cls);
    }
    return shouldHave;
  }
}

class StyleDeclaration {
  constructor(element) {
    this._el = element;
    this._styles = new Map();
  }

  setProperty(prop, value) {
    this._styles.set(prop, String(value));
  }

  getPropertyValue(prop) {
    return this._styles.get(prop) || '';
  }

  removeProperty(prop) {
    this._styles.delete(prop);
  }
}

export class DOMElement extends DOMNode {
  constructor(tagName) {
    super(1, tagName.toUpperCase());
    this.tagName = tagName.toUpperCase();
    this.attributes = new Map();
    this.classList = new ClassList(this);
    this.eventListeners = new Map();
    this._styleDecl = new StyleDeclaration(this);

    // Style proxy to allow direct property assignment like el.style.width = '30%'
    this.style = new Proxy(this._styleDecl, {
      get: (target, prop) => {
        if (prop in target || typeof target[prop] === 'function') {
          return target[prop];
        }
        return target.getPropertyValue(camelToKebab(prop));
      },
      set: (target, prop, value) => {
        target.setProperty(camelToKebab(prop), value);
        return true;
      }
    });

    this._rect = { left: 0, top: 0, width: 300, height: 100, right: 300, bottom: 100 };
  }

  get id() {
    return this.getAttribute('id') || '';
  }

  set id(val) {
    this.setAttribute('id', val);
  }

  get className() {
    return this.getAttribute('class') || '';
  }

  set className(val) {
    this.setAttribute('class', val);
  }

  getAttribute(name) {
    return this.attributes.has(name.toLowerCase()) ? this.attributes.get(name.toLowerCase()) : null;
  }

  setAttribute(name, val) {
    this.attributes.set(name.toLowerCase(), String(val));
  }

  hasAttribute(name) {
    return this.attributes.has(name.toLowerCase());
  }

  removeAttribute(name) {
    this.attributes.delete(name.toLowerCase());
  }

  getBoundingClientRect() {
    return { ...this._rect };
  }

  setBoundingClientRect(rect) {
    this._rect = { ...this._rect, ...rect };
  }

  setPointerCapture() {}
  releasePointerCapture() {}

  addEventListener(type, listener) {
    if (!this.eventListeners.has(type)) {
      this.eventListeners.set(type, []);
    }
    this.eventListeners.get(type).push(listener);
  }

  removeEventListener(type, listener) {
    if (!this.eventListeners.has(type)) return;
    const list = this.eventListeners.get(type).filter(l => l !== listener);
    this.eventListeners.set(type, list);
  }

  dispatchEvent(event) {
    event.target = this;
    event.currentTarget = this;
    const list = this.eventListeners.get(event.type) || [];
    for (const listener of [...list]) {
      listener.call(this, event);
    }
    if (!event.propagationStopped && this.parentNode && typeof this.parentNode.dispatchEvent === 'function') {
      this.parentNode.dispatchEvent(event);
    }
    return !event.defaultPrevented;
  }

  get textContent() {
    return this.childNodes.map(n => n.nodeType === 3 ? n.text : n.textContent).join('');
  }

  set textContent(str) {
    this.childNodes = [];
    if (str !== '') {
      this.appendChild(new DOMTextNode(String(str)));
    }
  }

  get innerHTML() {
    return serializeNodes(this.childNodes);
  }

  set innerHTML(htmlStr) {
    this.childNodes = [];
    if (!htmlStr) return;
    const parsed = parseHtmlFragment(String(htmlStr));
    for (const child of [...parsed]) {
      this.appendChild(child);
    }
  }

  matches(selector) {
    return matchesSelector(this, selector);
  }

  closest(selector) {
    let curr = this;
    while (curr && curr.nodeType === 1) {
      if (curr.matches(selector)) return curr;
      curr = curr.parentNode;
    }
    return null;
  }

  querySelector(selector) {
    return querySelectorOne(this, selector);
  }

  querySelectorAll(selector) {
    return querySelectorAll(this, selector);
  }
}

export class DOMTextNode extends DOMNode {
  constructor(text) {
    super(3, '#text');
    this.text = text;
  }

  get textContent() {
    return this.text;
  }

  set textContent(val) {
    this.text = String(val);
  }
}

export class DOMDocument extends DOMNode {
  constructor() {
    super(9, '#document');
    this.documentElement = new DOMElement('html');
    this.body = new DOMElement('body');
    this.head = new DOMElement('head');
    this.documentElement.appendChild(this.head);
    this.documentElement.appendChild(this.body);
    this.appendChild(this.documentElement);
    this.eventListeners = new Map();
  }

  createElement(tagName) {
    return new DOMElement(tagName);
  }

  createTextNode(text) {
    return new DOMTextNode(text);
  }

  getElementById(id) {
    return findFirst(this.documentElement, el => el.getAttribute('id') === id);
  }

  querySelector(selector) {
    return this.documentElement.querySelector(selector);
  }

  querySelectorAll(selector) {
    return this.documentElement.querySelectorAll(selector);
  }

  addEventListener(type, listener) {
    if (!this.eventListeners.has(type)) this.eventListeners.set(type, []);
    this.eventListeners.get(type).push(listener);
  }

  removeEventListener(type, listener) {
    if (!this.eventListeners.has(type)) return;
    this.eventListeners.set(type, this.eventListeners.get(type).filter(l => l !== listener));
  }

  dispatchEvent(event) {
    event.target = this;
    event.currentTarget = this;
    const list = this.eventListeners.get(event.type) || [];
    for (const listener of list) listener.call(this, event);
    return !event.defaultPrevented;
  }
}

function camelToKebab(str) {
  return str.replace(/([A-Z])/g, '-$1').toLowerCase();
}

function serializeNodes(nodes) {
  let out = '';
  for (const n of nodes) {
    if (n.nodeType === 3) {
      out += n.text;
    } else if (n.nodeType === 1) {
      const tag = n.tagName.toLowerCase();
      out += `<${tag}`;
      for (const [k, v] of n.attributes.entries()) {
        out += ` ${k}="${v}"`;
      }
      out += `>${n.innerHTML}</${tag}>`;
    }
  }
  return out;
}

function findFirst(root, predicate) {
  if (root.nodeType === 1 && predicate(root)) return root;
  for (const child of root.childNodes) {
    const found = findFirst(child, predicate);
    if (found) return found;
  }
  return null;
}

function matchesSelector(el, selector) {
  selector = selector.trim();
  if (selector.startsWith('#')) {
    return el.getAttribute('id') === selector.slice(1);
  }
  if (selector.startsWith('.')) {
    return el.classList.contains(selector.slice(1));
  }
  if (selector.startsWith('[') && selector.endsWith(']')) {
    const inside = selector.slice(1, -1);
    const eqIdx = inside.indexOf('=');
    if (eqIdx === -1) {
      return el.hasAttribute(inside);
    }
    const attr = inside.slice(0, eqIdx);
    let val = inside.slice(eqIdx + 1);
    if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
      val = val.slice(1, -1);
    }
    return el.getAttribute(attr) === val;
  }
  // Compound selector e.g. .cal-cell.today
  if (selector.includes('.') && !selector.includes(' ')) {
    const parts = selector.split('.').filter(Boolean);
    const firstIsTag = !selector.startsWith('.');
    if (firstIsTag && el.tagName.toLowerCase() !== parts[0].toLowerCase()) return false;
    const classes = firstIsTag ? parts.slice(1) : parts;
    return classes.every(c => el.classList.contains(c));
  }
  // Tag selector
  return el.tagName.toLowerCase() === selector.toLowerCase();
}

function querySelectorOne(root, selector) {
  const parts = selector.split(/\s+/).filter(Boolean);
  if (parts.length === 1) {
    return findFirst(root, el => matchesSelector(el, parts[0]));
  }
  const all = querySelectorAll(root, selector);
  return all.length > 0 ? all[0] : null;
}

function querySelectorAll(root, selector) {
  const results = [];
  const parts = selector.split(/\s+/).filter(Boolean);

  if (parts.length === 1) {
    const sel = parts[0];
    function walk(node) {
      for (const child of node.childNodes) {
        if (child.nodeType === 1) {
          if (matchesSelector(child, sel)) results.push(child);
          walk(child);
        }
      }
    }
    walk(root);
    return results;
  }

  // Two-level selector e.g. "div button" or ".item-title-grp .item-title"
  if (parts.length === 2) {
    const [ancestorSel, targetSel] = parts;
    const ancestors = querySelectorAll(root, ancestorSel);
    for (const anc of ancestors) {
      const descendants = querySelectorAll(anc, targetSel);
      for (const d of descendants) {
        if (!results.includes(d)) results.push(d);
      }
    }
    return results;
  }

  return results;
}

/**
 * Robust HTML fragment parser for concept markup
 */
export function parseHtmlFragment(html) {
  const root = new DOMNode(1, 'FRAGMENT');
  let current = root;
  const tagRegex = /<\/?([a-zA-Z0-9\-]+)([^>]*)>|([^<]+)/g;
  let match;

  const VOID_TAGS = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'param', 'source', 'track', 'wbr']);

  while ((match = tagRegex.exec(html)) !== null) {
    const [full, tagName, attrStr, text] = match;
    if (text) {
      current.appendChild(new DOMTextNode(text));
    } else if (full.startsWith('</')) {
      if (current.parentNode && current !== root) {
        current = current.parentNode;
      }
    } else if (tagName) {
      const el = new DOMElement(tagName);
      // parse attributes
      const attrRegex = /([a-zA-Z0-9\-:]+)(?:=["']([^"']*)["'])?/g;
      let aMatch;
      while ((aMatch = attrRegex.exec(attrStr)) !== null) {
        const attrName = aMatch[1];
        const attrVal = aMatch[2] !== undefined ? aMatch[2] : '';
        el.setAttribute(attrName, attrVal);
      }
      current.appendChild(el);

      const isSelfClosing = attrStr.trim().endsWith('/') || VOID_TAGS.has(tagName.toLowerCase());
      if (!isSelfClosing) {
        current = el;
      }
    }
  }

  return [...root.childNodes];
}

export class CustomEvent {
  constructor(type, eventInitDict = {}) {
    this.type = type;
    this.bubbles = !!eventInitDict.bubbles;
    this.cancelable = !!eventInitDict.cancelable;
    this.detail = eventInitDict.detail || null;
    this.clientX = eventInitDict.clientX || 0;
    this.clientY = eventInitDict.clientY || 0;
    this.key = eventInitDict.key || '';
    this.target = null;
    this.currentTarget = null;
    this.propagationStopped = false;
    this.defaultPrevented = false;
  }

  stopPropagation() {
    this.propagationStopped = true;
  }

  preventDefault() {
    this.defaultPrevented = true;
  }
}

/**
 * Sandboxed Environment for dynamic-island-concept.html
 */
export function loadConceptApp(customHtml = null) {
  const html = customHtml || fs.readFileSync(CONCEPT_HTML_PATH, 'utf8');

  // Extract <style>
  const styleMatch = html.match(/<style[^>]*>([\s\S]*?)<\/style>/i);
  const css = styleMatch ? styleMatch[1] : '';

  // Extract <script>
  const scriptMatch = html.match(/<script[^>]*>([\s\S]*?)<\/script>/i);
  const scriptCode = scriptMatch ? scriptMatch[1] : '';

  // Build document
  const doc = new DOMDocument();

  // Extract html attributes
  const htmlTagMatch = html.match(/<html([^>]*)>/i);
  if (htmlTagMatch) {
    const attrRegex = /([a-zA-Z0-9\-:]+)(?:=["']([^"']*)["'])?/g;
    let a;
    while ((a = attrRegex.exec(htmlTagMatch[1])) !== null) {
      doc.documentElement.setAttribute(a[1], a[2] || '');
    }
  }

  // Extract body content
  const bodyMatch = html.match(/<body[^>]*>([\s\S]*?)<\/body>/i);
  const bodyInner = bodyMatch ? bodyMatch[1].replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '') : '';
  const parsedBodyNodes = parseHtmlFragment(bodyInner);
  for (const node of [...parsedBodyNodes]) {
    doc.body.appendChild(node);
  }

  const activeTimers = new Set();
  const safeSetTimeout = (fn, ms) => {
    const t = setTimeout(() => {
      activeTimers.delete(t);
      fn();
    }, ms);
    if (t.unref) t.unref();
    activeTimers.add(t);
    return t;
  };
  const safeClearTimeout = (id) => {
    clearTimeout(id);
    activeTimers.delete(id);
  };
  const safeSetInterval = (fn, ms) => {
    const t = setInterval(fn, ms);
    if (t.unref) t.unref();
    activeTimers.add(t);
    return t;
  };
  const safeClearInterval = (id) => {
    clearInterval(id);
    activeTimers.delete(id);
  };

  // Window object
  const win = {
    document: doc,
    navigator: { userAgent: 'NodeTestRunner' },
    Event: CustomEvent,
    CustomEvent: CustomEvent,
    MouseEvent: CustomEvent,
    KeyboardEvent: CustomEvent,
    Date: Date,
    Math: Math,
    String: String,
    setTimeout: safeSetTimeout,
    clearTimeout: safeClearTimeout,
    setInterval: safeSetInterval,
    clearInterval: safeClearInterval,
    addEventListener: (type, listener) => doc.addEventListener(type, listener),
    removeEventListener: (type, listener) => doc.removeEventListener(type, listener),
    dispatchEvent: (e) => doc.dispatchEvent(e)
  };

  win.window = win;

  // Run script inside VM
  const context = vm.createContext({
    window: win,
    document: doc,
    Event: CustomEvent,
    CustomEvent: CustomEvent,
    MouseEvent: CustomEvent,
    KeyboardEvent: CustomEvent,
    Date: Date,
    Math: Math,
    String: String,
    setTimeout: win.setTimeout,
    clearTimeout: win.clearTimeout,
    setInterval: win.setInterval,
    clearInterval: win.clearInterval,
    console: console
  });

  if (scriptCode) {
    try {
      vm.runInContext(scriptCode, context);
    } catch (err) {
      // Allow caller to inspect script errors
      context._scriptError = err;
    }
  }

  return {
    window: win,
    document: doc,
    css,
    html,
    scriptCode,
    scriptError: context._scriptError || null,

    // Helper interactions
    click(target, init = {}) {
      if (!target) throw new Error('Target element for click does not exist');
      const ev = new CustomEvent('click', { bubbles: true, cancelable: true, ...init });
      target.dispatchEvent(ev);
      return ev;
    },

    keydown(key, init = {}) {
      const ev = new CustomEvent('keydown', { bubbles: true, cancelable: true, key, ...init });
      win.dispatchEvent(ev);
      return ev;
    }
  };
}

/**
 * CSS Static Token & Rule Analyzer Utilities
 */
export function analyzeCss(css) {
  // Extract CSS variables from :root or selectors
  const varMap = new Map();
  const varRegex = /--([a-zA-Z0-9\-]+)\s*:\s*([^;]+);/g;
  let vmMatch;
  while ((vmMatch = varRegex.exec(css)) !== null) {
    varMap.set(`--${vmMatch[1]}`, vmMatch[2].trim());
  }

  // Extract all border-radius values
  const radiiMatches = [];
  const radiusRegex = /border-radius\s*:\s*([^;]+);/g;
  let rMatch;
  while ((rMatch = radiusRegex.exec(css)) !== null) {
    radiiMatches.push(rMatch[1].trim());
  }

  // Find intermediate radii (e.g. 6px, 10px, 14px, 16px, 20px)
  const forbiddenIntermediateRadii = [];
  const intermediatePattern = /\b(2px|3px|5px|6px|7px|9px|10px|11px|13px|14px|15px|16px|17px|20px|24px)\b/i;
  for (const r of radiiMatches) {
    if (intermediatePattern.test(r)) {
      forbiddenIntermediateRadii.push(r);
    }
  }

  // Check keyframes
  const keyframes = [];
  const kfRegex = /@keyframes\s+([a-zA-Z0-9\-_]+)\s*\{([^}]+(?:\{[^}]+\}[^}]+)*)\}/g;
  let kMatch;
  while ((kMatch = kfRegex.exec(css)) !== null) {
    keyframes.push({ name: kMatch[1], content: kMatch[2] });
  }

  // Check uppercase rules
  const uppercaseRules = [];
  const ruleRegex = /([^{}]+)\{([^}]+)\}/g;
  let ruleMatch;
  while ((ruleMatch = ruleRegex.exec(css)) !== null) {
    const sel = ruleMatch[1].trim();
    const body = ruleMatch[2];
    if (/text-transform\s*:\s*uppercase/i.test(body)) {
      uppercaseRules.push(sel);
    }
  }

  return {
    cssVariables: varMap,
    allRadiiDeclarations: radiiMatches,
    forbiddenIntermediateRadii,
    keyframes,
    uppercaseRules
  };
}
