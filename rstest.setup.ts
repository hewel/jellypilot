import { expect } from '@rstest/core';
import * as jestDomMatchers from '@testing-library/jest-dom/matchers';

expect.extend(jestDomMatchers);

class TestResizeObserver {
  readonly #callback: ResizeObserverCallback;

  constructor(callback: ResizeObserverCallback) {
    this.#callback = callback;
  }

  observe() {
    // Real ResizeObserver fires an initial notification on observe(); mount measurement depends on it.
    this.#callback([], this);
  }

  unobserve() {}

  disconnect() {}
}

class TestIntersectionObserver implements IntersectionObserver {
  static instances = new Set<TestIntersectionObserver>();

  readonly root = null;
  readonly rootMargin = '';
  readonly scrollMargin = '';
  readonly thresholds = [];
  private readonly callback: IntersectionObserverCallback;
  private readonly elements = new Set<Element>();

  constructor(callback: IntersectionObserverCallback) {
    this.callback = callback;
    TestIntersectionObserver.instances.add(this);
  }

  observe(element: Element) {
    this.elements.add(element);
  }

  unobserve(element: Element) {
    this.elements.delete(element);
  }

  disconnect() {
    this.elements.clear();
    TestIntersectionObserver.instances.delete(this);
  }

  takeRecords() {
    return [];
  }

  static trigger(isIntersecting = true) {
    for (const instance of TestIntersectionObserver.instances) {
      const entries = [...instance.elements].map((element) => ({
        boundingClientRect: element.getBoundingClientRect(),
        intersectionRatio: isIntersecting ? 1 : 0,
        intersectionRect: isIntersecting ? element.getBoundingClientRect() : new DOMRect(),
        isIntersecting,
        rootBounds: null,
        target: element,
        time: performance.now(),
      }));
      instance.callback(entries, instance);
    }
  }
}

const storePathsByRid = new Map<number, string>();
const storeRidsByPath = new Map<string, number>();
const storeValuesByPath = new Map<string, Map<string, unknown>>();
const storeLoadCountsByPath = new Map<string, number>();
const storeGetCountsByPathKey = new Map<string, number>();
let nextStoreRid = 1;

function record(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === 'object' ? (value as Record<string, unknown>) : {};
}

function storeForPath(path: string) {
  let store = storeValuesByPath.get(path);
  if (!store) {
    store = new Map();
    storeValuesByPath.set(path, store);
  }
  return store;
}

function loadTestStore(path: string) {
  storeLoadCountsByPath.set(path, (storeLoadCountsByPath.get(path) ?? 0) + 1);
  const existingRid = storeRidsByPath.get(path);
  if (existingRid) {
    return existingRid;
  }
  const rid = nextStoreRid;
  nextStoreRid += 1;
  storeRidsByPath.set(path, rid);
  storePathsByRid.set(rid, path);
  storeForPath(path);
  return rid;
}

const testTauriStore = {
  reset() {
    storePathsByRid.clear();
    storeRidsByPath.clear();
    storeValuesByPath.clear();
    storeLoadCountsByPath.clear();
    storeGetCountsByPathKey.clear();
    nextStoreRid = 1;
  },
  get(path: string, key: string) {
    return storeValuesByPath.get(path)?.get(key);
  },
  getCount(path: string, key: string) {
    return storeGetCountsByPathKey.get(`${path}\u0000${key}`) ?? 0;
  },
  loadCount(path: string) {
    return storeLoadCountsByPath.get(path) ?? 0;
  },
  set(path: string, key: string, value: unknown) {
    storeForPath(path).set(key, value);
  },
};

declare global {
  interface Window {
    __TEST_INTERSECTION_OBSERVER__: typeof TestIntersectionObserver;
    __TEST_TAURI_STORE__: typeof testTauriStore;
  }
}

Object.assign(window, {
  IntersectionObserver: TestIntersectionObserver,
  ResizeObserver: TestResizeObserver,
  __TEST_INTERSECTION_OBSERVER__: TestIntersectionObserver,
});

Object.assign(window, {
  __TEST_TAURI_STORE__: testTauriStore,
  __TAURI_EVENT_PLUGIN_INTERNALS__: {
    unregisterListener: () => {},
  },
  __TAURI_INTERNALS__: {
    convertFileSrc: (path: string, protocol = 'asset') => `${protocol}://localhost/${path}`,
    invoke: async (cmd: string, args?: unknown) => {
      if (cmd === 'plugin:app|version') {
        return 'test';
      }
      if (cmd === 'plugin:event|listen') {
        return 1;
      }
      if (cmd === 'plugin:store|load') {
        const path = record(args).path;
        return typeof path === 'string' ? loadTestStore(path) : 0;
      }
      if (cmd === 'plugin:store|get_store') {
        const path = record(args).path;
        return typeof path === 'string' ? (storeRidsByPath.get(path) ?? null) : null;
      }
      if (cmd === 'plugin:store|get') {
        const argsRecord = record(args);
        const path = typeof argsRecord.rid === 'number' ? storePathsByRid.get(argsRecord.rid) : null;
        const key = argsRecord.key;
        if (!path || typeof key !== 'string') {
          return [null, false];
        }
        storeGetCountsByPathKey.set(
          `${path}\u0000${key}`,
          (storeGetCountsByPathKey.get(`${path}\u0000${key}`) ?? 0) + 1,
        );
        const store = storeForPath(path);
        return store.has(key) ? [store.get(key), true] : [null, false];
      }
      if (cmd === 'plugin:store|set') {
        const argsRecord = record(args);
        const path = typeof argsRecord.rid === 'number' ? storePathsByRid.get(argsRecord.rid) : null;
        const key = argsRecord.key;
        if (path && typeof key === 'string') {
          storeForPath(path).set(key, argsRecord.value);
        }
        return null;
      }
      if (cmd === 'plugin:store|save') {
        return null;
      }
      return null;
    },
    transformCallback: () => 1,
    unregisterCallback: () => undefined,
  },
});
