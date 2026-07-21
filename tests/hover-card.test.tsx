import { afterEach, beforeEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { HoverCard } from '../src/components/ui/HoverCard';

const mounted: (() => void)[] = [];

function renderCard(props: { onOpenChange?: (open: boolean) => void } = {}) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <HoverCard onOpenChange={props.onOpenChange} content={() => <p>Card body</p>}>
        <button type="button">Trigger</button>
      </HoverCard>
    ),
    root,
  );
  mounted.push(() => {
    dispose();
    root.remove();
  });
  return { root, wrapper: root.firstElementChild as HTMLElement };
}

beforeEach(() => {
  rstest.useFakeTimers();
});

afterEach(() => {
  while (mounted.length > 0) {
    mounted.pop()?.();
  }
  document.body.innerHTML = '';
  rstest.useRealTimers();
});

test('opens on hover after the open delay, not before', () => {
  const { wrapper } = renderCard();

  fireEvent.pointerEnter(wrapper);
  rstest.advanceTimersByTime(249);
  expect(screen.queryByText('Card body')).not.toBeInTheDocument();

  rstest.advanceTimersByTime(1);
  expect(screen.getByText('Card body')).toBeInTheDocument();
});

test('does not open when the pointer leaves before the open delay', () => {
  const { wrapper } = renderCard();

  fireEvent.pointerEnter(wrapper);
  rstest.advanceTimersByTime(100);
  fireEvent.pointerLeave(wrapper);
  rstest.advanceTimersByTime(1000);
  expect(screen.queryByText('Card body')).not.toBeInTheDocument();
});

test('closes after the close delay once the pointer leaves trigger and content', () => {
  const { wrapper } = renderCard();

  fireEvent.pointerEnter(wrapper);
  rstest.advanceTimersByTime(250);
  const card = screen.getByText('Card body').parentElement as HTMLElement;

  fireEvent.pointerLeave(wrapper);
  fireEvent.pointerEnter(card);
  rstest.advanceTimersByTime(1000);
  expect(screen.getByText('Card body')).toBeInTheDocument();

  fireEvent.pointerLeave(card);
  rstest.advanceTimersByTime(299);
  expect(screen.getByText('Card body')).toBeInTheDocument();
  rstest.advanceTimersByTime(1);
  expect(screen.queryByText('Card body')).not.toBeInTheDocument();
});

test('opens immediately on keyboard focus and closes on Escape', () => {
  renderCard();
  const trigger = screen.getByText('Trigger');

  fireEvent.focusIn(trigger);
  expect(screen.getByText('Card body')).toBeInTheDocument();

  fireEvent.keyDown(document, { key: 'Escape' });
  expect(screen.queryByText('Card body')).not.toBeInTheDocument();
});

test('closes immediately on any document scroll', () => {
  const { wrapper } = renderCard();

  fireEvent.pointerEnter(wrapper);
  rstest.advanceTimersByTime(250);
  expect(screen.getByText('Card body')).toBeInTheDocument();

  const scroller = document.createElement('div');
  document.body.append(scroller);
  scroller.dispatchEvent(new Event('scroll'));
  expect(screen.queryByText('Card body')).not.toBeInTheDocument();
});

test('reports open transitions through onOpenChange', () => {
  const transitions: boolean[] = [];
  const { wrapper } = renderCard({ onOpenChange: (open) => transitions.push(open) });

  fireEvent.pointerEnter(wrapper);
  rstest.advanceTimersByTime(250);
  fireEvent.pointerLeave(wrapper);
  rstest.advanceTimersByTime(300);

  expect(transitions).toEqual([true, false]);
});
