// @rstest-environment jsdom
import { afterEach, expect, test } from '@rstest/core';
import { fireEvent } from '@testing-library/dom';
import { createRenderEffect } from 'solid-js';
import { render } from 'solid-js/web';

import { createAmbientGlow } from '../src/utils/ambientGlow';

const mounted: (() => void)[] = [];

interface ShellFixture {
  shell: HTMLElement;
  sidebar: HTMLElement;
  sidebarInner: HTMLElement;
  sidebarButton: HTMLElement;
  toolbar: HTMLElement;
  main: HTMLElement;
}

function renderShell(): ShellFixture {
  const root = document.createElement('div');
  document.body.append(root);
  let fixture!: ShellFixture;
  const dispose = render(() => {
    const glow = createAmbientGlow();

    const shell = document.createElement('div');
    shell.dataset.shell = '';
    shell.addEventListener('pointerover', glow.onPointerOver);
    shell.addEventListener('pointerout', glow.onPointerOut);
    createRenderEffect(() => {
      if (glow.active()) {
        shell.dataset.glow = '';
      } else {
        delete shell.dataset.glow;
      }
    });

    const sidebar = document.createElement('aside');
    sidebar.dataset.sidebar = '';
    const sidebarInner = document.createElement('div');
    const sidebarButton = document.createElement('button');
    sidebarButton.type = 'button';
    sidebarButton.textContent = 'Libraries';
    sidebarInner.append(sidebarButton);
    sidebar.append(sidebarInner);

    const toolbar = document.createElement('nav');
    toolbar.dataset.toolbar = '';

    const main = document.createElement('main');

    shell.append(sidebar, toolbar, main);
    fixture = { shell, sidebar, sidebarInner, sidebarButton, toolbar, main };
    return shell;
  }, root);
  mounted.push(() => {
    dispose();
    root.remove();
  });
  return fixture;
}

afterEach(() => {
  while (mounted.length > 0) {
    mounted.pop()?.();
  }
  document.body.innerHTML = '';
});

test('sets data-glow when the pointer moves over the sidebar', () => {
  const { shell, sidebarButton } = renderShell();

  expect(shell).not.toHaveAttribute('data-glow');
  fireEvent.pointerOver(sidebarButton);
  expect(shell).toHaveAttribute('data-glow');
});

test('keeps data-glow when the pointer moves between sidebar children', () => {
  const { shell, sidebarButton, sidebarInner } = renderShell();

  fireEvent.pointerOver(sidebarButton);
  fireEvent.pointerOut(sidebarButton, { relatedTarget: sidebarInner });
  expect(shell).toHaveAttribute('data-glow');
});

test('keeps data-glow when the pointer moves from the sidebar to the toolbar', () => {
  const { shell, sidebar, toolbar } = renderShell();

  fireEvent.pointerOver(sidebar);
  fireEvent.pointerOut(sidebar, { relatedTarget: toolbar });
  expect(shell).toHaveAttribute('data-glow');
});

test('clears data-glow when the pointer moves from the toolbar to main content', () => {
  const { shell, toolbar, main } = renderShell();

  fireEvent.pointerOver(toolbar);
  expect(shell).toHaveAttribute('data-glow');
  fireEvent.pointerOut(toolbar, { relatedTarget: main });
  expect(shell).not.toHaveAttribute('data-glow');
});

test('clears data-glow when the pointer leaves the window from the toolbar', () => {
  const { shell, toolbar } = renderShell();

  fireEvent.pointerOver(toolbar);
  expect(shell).toHaveAttribute('data-glow');
  fireEvent.pointerOut(toolbar, { relatedTarget: null });
  expect(shell).not.toHaveAttribute('data-glow');
});

test('clears data-glow on the next pointerover after the hovered region unmounts', () => {
  const { shell, toolbar, main } = renderShell();

  fireEvent.pointerOver(toolbar);
  expect(shell).toHaveAttribute('data-glow');
  toolbar.remove();
  fireEvent.pointerOver(main);
  expect(shell).not.toHaveAttribute('data-glow');
});

test('keeps data-glow until every pointer has left the glow regions', () => {
  const { shell, sidebar, toolbar } = renderShell();

  fireEvent.pointerOver(sidebar, { pointerId: 1 });
  fireEvent.pointerOver(toolbar, { pointerId: 2 });
  fireEvent.pointerOut(sidebar, { pointerId: 1, relatedTarget: null });
  expect(shell).toHaveAttribute('data-glow');
  fireEvent.pointerOut(toolbar, { pointerId: 2, relatedTarget: null });
  expect(shell).not.toHaveAttribute('data-glow');
});
