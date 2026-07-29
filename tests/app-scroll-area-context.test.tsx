// @rstest-environment jsdom
import { expect, test } from '@rstest/core';
import { fireEvent, screen } from '@testing-library/dom';
import { render } from 'solid-js/web';

import {
  AppScrollAreaProvider,
  createAppScrollAreaController,
  createAppScrollListener,
  useAppScrollArea,
} from '../src/components/AppScrollAreaContext';
import type { AppScrollSnapshot } from '../src/components/AppScrollAreaContext';

function defineScrollMetrics(
  viewport: HTMLElement,
  metrics: Pick<AppScrollSnapshot, 'clientHeight' | 'clientWidth' | 'scrollHeight' | 'scrollWidth'>,
) {
  Object.defineProperties(viewport, {
    clientHeight: { configurable: true, value: metrics.clientHeight },
    clientWidth: { configurable: true, value: metrics.clientWidth },
    scrollHeight: { configurable: true, value: metrics.scrollHeight },
    scrollWidth: { configurable: true, value: metrics.scrollWidth },
  });
}

function trackScrollMetricReads(
  viewport: HTMLElement,
  metrics: Pick<AppScrollSnapshot, 'clientHeight' | 'clientWidth' | 'scrollHeight' | 'scrollWidth'>,
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

function ScrollConsumer(props: { onSnapshot: (snapshot: AppScrollSnapshot) => void }) {
  const appScroll = useAppScrollArea();
  createAppScrollListener((snapshot) => props.onSnapshot(snapshot));

  return <span data-testid="scroll-top">{appScroll.snapshot().scrollTop}</span>;
}

function TestScrollArea(props: {
  onController?: (appScroll: ReturnType<typeof createAppScrollAreaController>) => void;
  onSnapshot: (snapshot: AppScrollSnapshot) => void;
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
        <ScrollConsumer onSnapshot={props.onSnapshot} />
      </div>
    </AppScrollAreaProvider>
  );
}

test('app scroll area context publishes viewport scroll snapshots to descendants', () => {
  let appScroll: ReturnType<typeof createAppScrollAreaController> | undefined;
  const snapshots: AppScrollSnapshot[] = [];
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestScrollArea
        onController={(controller) => {
          appScroll = controller;
        }}
        onSnapshot={(snapshot) => snapshots.push(snapshot)}
      />
    ),
    root,
  );

  const viewport = screen.getByTestId('app-scroll-viewport');
  defineScrollMetrics(viewport, {
    clientHeight: 100,
    clientWidth: 240,
    scrollHeight: 300,
    scrollWidth: 240,
  });
  appScroll?.measure();
  viewport.scrollTop = 120;

  fireEvent.scroll(viewport);

  expect(snapshots.at(-1)?.scrollTop).toBe(120);
  expect(snapshots.at(-1)?.atBottom).toBe(false);
  expect(screen.getByTestId('scroll-top')).toHaveTextContent('120');

  dispose();
  root.remove();
});

test('app scroll area context does not read layout geometry during scroll events', () => {
  let appScroll: ReturnType<typeof createAppScrollAreaController> | undefined;
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestScrollArea
        onController={(controller) => {
          appScroll = controller;
        }}
        onSnapshot={() => undefined}
      />
    ),
    root,
  );

  const viewport = screen.getByTestId('app-scroll-viewport');
  const metricReads = trackScrollMetricReads(viewport, {
    clientHeight: 100,
    clientWidth: 240,
    scrollHeight: 300,
    scrollWidth: 240,
  });
  appScroll?.measure();
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
