import { Dialog, jellypilotTheme, Menu, Selector, Theme, UIRoot } from '@jellypilot/ui';
import { afterEach, expect, test } from '@rstest/core';
import { fireEvent, screen, waitFor, within } from '@testing-library/dom';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';

function mount(view: () => unknown) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(view as () => never, root);
  return () => {
    dispose();
    root.remove();
  };
}

test('Menu and Selector provide roving keyboard collection navigation', async () => {
  const menuSelections: string[] = [];
  const selectorSelections: string[] = [];
  const dispose = mount(() => (
    <UIRoot>
      <Menu
        trigger="Actions"
        items={[
          { label: 'First', value: 'first' },
          { label: 'Unavailable', value: 'unavailable', disabled: true },
          { label: 'Last', value: 'last' },
        ]}
        onSelect={(value) => menuSelections.push(value)}
      />
      <Selector
        aria-label="Quality"
        value="medium"
        items={[
          { label: 'Low', value: 'low' },
          { label: 'Medium', value: 'medium' },
          { label: 'High', value: 'high' },
        ]}
        onValueChange={(value) => {
          if (value) selectorSelections.push(value);
        }}
      />
    </UIRoot>
  ));

  const menuTrigger = screen.getByRole('button', { name: 'Actions' });
  fireEvent.keyDown(menuTrigger, { key: 'ArrowDown' });
  await waitFor(() => expect(screen.getByRole('menuitem', { name: 'First' })).toHaveFocus());
  fireEvent.keyDown(document.activeElement!, { key: 'End' });
  expect(screen.getByRole('menuitem', { name: 'Last' })).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: 'Enter' });
  expect(menuSelections).toEqual(['last']);
  expect(menuTrigger).toHaveFocus();

  const selectorTrigger = screen.getByRole('button', { name: 'Quality' });
  fireEvent.keyDown(selectorTrigger, { key: 'ArrowDown' });
  await waitFor(() => expect(screen.getByRole('option', { name: 'Medium' })).toHaveFocus());
  fireEvent.keyDown(document.activeElement!, { key: 'Home' });
  expect(screen.getByRole('option', { name: 'Low' })).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: ' ' });
  expect(selectorSelections).toEqual(['low']);
  expect(selectorTrigger).toHaveFocus();

  dispose();
});

test('only the topmost dialog dismisses and modal focus remains trapped', async () => {
  const [outerOpen, setOuterOpen] = createSignal(true);
  const [innerOpen, setInnerOpen] = createSignal(true);
  const dispose = mount(() => (
    <UIRoot>
      <Dialog open={outerOpen()} title="Outer" onOpenChange={setOuterOpen}>
        <button type="button">Outer action</button>
      </Dialog>
      <Dialog open={innerOpen()} title="Inner" onOpenChange={setInnerOpen}>
        <button type="button">First action</button>
        <button type="button">Last action</button>
      </Dialog>
    </UIRoot>
  ));

  await waitFor(() => expect(screen.getByRole('button', { name: 'First action' })).toHaveFocus());
  expect(document.querySelector('[data-jp-uiroot]')).toHaveAttribute('inert');
  expect(document.body.style.overflow).toBe('hidden');
  within(screen.getByRole('dialog', { name: 'Inner' }))
    .getByRole('button', { name: 'Close' })
    .focus();
  fireEvent.keyDown(document.activeElement!, { key: 'Tab' });
  expect(screen.getByRole('button', { name: 'First action' })).toHaveFocus();

  fireEvent.keyDown(document, { key: 'Escape' });
  expect(screen.queryByRole('dialog', { name: 'Inner' })).toBeNull();
  expect(screen.getByRole('dialog', { name: 'Outer' })).toBeVisible();
  fireEvent.keyDown(document, { key: 'Escape' });
  expect(screen.queryByRole('dialog', { name: 'Outer' })).toBeNull();
  await waitFor(() =>
    expect(document.querySelector('[data-jp-uiroot]')).not.toHaveAttribute('inert'),
  );
  expect(document.body.style.overflow).toBe('');

  dispose();
});

test('nested theme descriptor and mode follow portaled dialog content', async () => {
  const dispose = mount(() => (
    <UIRoot preference="light">
      <Theme descriptor={jellypilotTheme} mode="dark">
        <Dialog open title="Nested theme">
          Content
        </Dialog>
      </Theme>
    </UIRoot>
  ));

  const dialog = await screen.findByRole('dialog', { name: 'Nested theme' });
  const portalTheme = dialog.closest('[data-jp-layer-portal]');
  expect(portalTheme).toHaveAttribute('data-theme', 'dark');
  expect(portalTheme).toHaveAttribute('data-theme-id', 'jellypilot');

  dispose();
});

afterEach(() => {
  document.body.innerHTML = '';
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.themeId;
});
