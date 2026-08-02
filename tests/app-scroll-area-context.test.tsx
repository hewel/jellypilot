// @rstest-environment jsdom
import { expect, test } from '@rstest/core';
import { fireEvent, screen } from '@testing-library/dom';
import { createEffect } from 'solid-js';
import { render } from 'solid-js/web';

import {
  AppScrollAreaProvider,
  createAppScrollAreaController,
  useAppScrollArea,
} from '../src/components/AppScrollAreaContext';
import type { AppScrollAreaApi } from '../src/components/AppScrollAreaContext';

function trackScrollMetricReads(
  viewport: HTMLElement,
  metrics: {
    clientHeight: number;
    clientWidth: number;
    scrollHeight: number;
    scrollWidth: number;
  },
) {
  const reads = {
    clientHeight: 0,
    clientWidth: 0,
    scrollHeight: 0,
    scrollWidth: 0,
  };

  Object.defineProperties(viewport, {
    clientHeight: {
      configurable: true,
      get: () => {
        reads.clientHeight += 1;
        return metrics.clientHeight;
      },
    },
    clientWidth: {
      configurable: true,
      get: () => {
        reads.clientWidth += 1;
        return metrics.clientWidth;
      },
    },
    scrollHeight: {
      configurable: true,
      get: () => {
        reads.scrollHeight += 1;
        return metrics.scrollHeight;
      },
    },
    scrollWidth: {
      configurable: true,
      get: () => {
        reads.scrollWidth += 1;
        return metrics.scrollWidth;
      },
    },
  });

  return {
    reads,
    reset: () => {
      reads.clientHeight = 0;
      reads.clientWidth = 0;
      reads.scrollHeight = 0;
      reads.scrollWidth = 0;
    },
  };
}

function ScrollConsumer() {
  const appScroll = useAppScrollArea();
  return <span data-testid="scrolled">{String(appScroll.scrolled())}</span>;
}

function ScrolledEffectConsumer(props: { onScrolledChange: (scrolled: boolean) => void }) {
  const appScroll = useAppScrollArea();

  createEffect(() => {
    props.onScrolledChange(appScroll.scrolled());
  });

  return null;
}

function TestScrollArea(props: {
  onController?: (appScroll: AppScrollAreaApi) => void;
  onScrolledChange?: (scrolled: boolean) => void;
}) {
  const appScroll = createAppScrollAreaController();
  props.onController?.(appScroll);

  return (
    <AppScrollAreaProvider value={appScroll}>
      <div
        data-testid="app-scroll-viewport"
        ref={appScroll.setViewport}
        onScroll={appScroll.handleViewportScroll}
      >
        <ScrollConsumer />
        {props.onScrolledChange ? (
          <ScrolledEffectConsumer onScrolledChange={props.onScrolledChange} />
        ) : null}
      </div>
    </AppScrollAreaProvider>
  );
}

test('app scroll area context publishes scrolled across the toolbar threshold', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <TestScrollArea />, root);

  const viewport = screen.getByTestId('app-scroll-viewport');
  expect(screen.getByTestId('scrolled')).toHaveTextContent('false');

  viewport.scrollTop = 5;
  fireEvent.scroll(viewport);
  expect(screen.getByTestId('scrolled')).toHaveTextContent('true');

  viewport.scrollTop = 4;
  fireEvent.scroll(viewport);
  expect(screen.getByTestId('scrolled')).toHaveTextContent('false');

  dispose();
  root.remove();
});

test('app scroll area context suppresses repeated scrolled notifications above threshold', () => {
  const scrolledValues: boolean[] = [];
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestScrollArea
        onScrolledChange={(scrolled) => {
          scrolledValues.push(scrolled);
        }}
      />
    ),
    root,
  );

  const viewport = screen.getByTestId('app-scroll-viewport');
  expect(scrolledValues).toEqual([false]);

  viewport.scrollTop = 5;
  fireEvent.scroll(viewport);
  viewport.scrollTop = 20;
  fireEvent.scroll(viewport);
  viewport.scrollTop = 40;
  fireEvent.scroll(viewport);

  expect(scrolledValues).toEqual([false, true]);

  viewport.scrollTop = 0;
  fireEvent.scroll(viewport);

  expect(scrolledValues).toEqual([false, true, false]);

  dispose();
  root.remove();
});

test('app scroll area context does not read layout geometry during scroll events', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <TestScrollArea />, root);

  const viewport = screen.getByTestId('app-scroll-viewport');
  const metricReads = trackScrollMetricReads(viewport, {
    clientHeight: 100,
    clientWidth: 240,
    scrollHeight: 300,
    scrollWidth: 240,
  });
  metricReads.reset();
  viewport.scrollTop = 120;

  fireEvent.scroll(viewport);

  expect(metricReads.reads).toEqual({
    clientHeight: 0,
    clientWidth: 0,
    scrollHeight: 0,
    scrollWidth: 0,
  });

  dispose();
  root.remove();
});

test('app scroll area context resets scrolled when viewport detaches', () => {
  let appScroll: AppScrollAreaApi | undefined;
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestScrollArea
        onController={(controller) => {
          appScroll = controller;
        }}
      />
    ),
    root,
  );

  const viewport = screen.getByTestId('app-scroll-viewport');
  viewport.scrollTop = 20;
  fireEvent.scroll(viewport);
  expect(screen.getByTestId('scrolled')).toHaveTextContent('true');

  appScroll?.setViewport(null);
  expect(appScroll?.scrolled()).toBe(false);
  expect(screen.getByTestId('scrolled')).toHaveTextContent('false');

  dispose();
  root.remove();
});

test('app scroll area context ignores stale scroll after viewport detach', () => {
  let appScroll: AppScrollAreaApi | undefined;
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestScrollArea
        onController={(controller) => {
          appScroll = controller;
        }}
      />
    ),
    root,
  );

  const oldViewport = screen.getByTestId('app-scroll-viewport');
  oldViewport.scrollTop = 20;
  fireEvent.scroll(oldViewport);
  expect(appScroll?.scrolled()).toBe(true);

  appScroll?.setViewport(null);
  expect(appScroll?.viewport()).toBeNull();
  expect(appScroll?.scrolled()).toBe(false);

  oldViewport.scrollTop = 40;
  fireEvent.scroll(oldViewport);

  expect(appScroll?.viewport()).toBeNull();
  expect(appScroll?.scrolled()).toBe(false);

  dispose();
  root.remove();
});

test('app scroll area context ignores stale scroll after viewport replace', () => {
  let appScroll: AppScrollAreaApi | undefined;
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestScrollArea
        onController={(controller) => {
          appScroll = controller;
        }}
      />
    ),
    root,
  );

  const oldViewport = screen.getByTestId('app-scroll-viewport');
  oldViewport.scrollTop = 20;
  fireEvent.scroll(oldViewport);
  expect(appScroll?.scrolled()).toBe(true);

  const nextViewport = document.createElement('div');
  nextViewport.scrollTop = 0;
  appScroll?.setViewport(nextViewport);

  expect(appScroll?.viewport()).toBe(nextViewport);
  expect(appScroll?.scrolled()).toBe(false);

  oldViewport.scrollTop = 80;
  fireEvent.scroll(oldViewport);

  expect(appScroll?.viewport()).toBe(nextViewport);
  expect(appScroll?.scrolled()).toBe(false);

  dispose();
  root.remove();
});
