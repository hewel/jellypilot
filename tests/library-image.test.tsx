// @rstest-environment jsdom
import { afterEach, beforeEach, expect, test } from '@rstest/core';
import { fireEvent, screen } from '@testing-library/dom';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';

import { LibraryImage } from '../src/components/library/LibraryImage';
import { resetImageProxyBase, setImageProxyBase } from '../src/utils/imageSource';

const PROXY_BASE = 'http://127.0.0.1:43127';

beforeEach(() => setImageProxyBase(PROXY_BASE));
afterEach(() => {
  resetImageProxyBase();
  document.body.innerHTML = '';
});

function renderImage(imageId: () => string | null) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => <LibraryImage imageId={imageId()} alt="Artwork" fallback={<span>No artwork</span>} />,
    root,
  );
  return { dispose, root };
}

test('renders the logical image reference through the localhost proxy', () => {
  const { dispose, root } = renderImage(() => 'signed-image');

  expect(screen.getByAltText('Artwork')).toHaveAttribute('src', `${PROXY_BASE}/image/signed-image`);

  dispose();
  root.remove();
});

test('first load error renders the fallback immediately', () => {
  const { dispose, root } = renderImage(() => 'broken-image');

  fireEvent.error(screen.getByAltText('Artwork'));

  expect(screen.queryByAltText('Artwork')).toBeNull();
  expect(screen.getByText('No artwork')).toBeVisible();

  dispose();
  root.remove();
});

test('changing the image reference allows one fresh load', () => {
  const [imageId, setImageId] = createSignal<string | null>('first-image');
  const { dispose, root } = renderImage(imageId);

  fireEvent.error(screen.getByAltText('Artwork'));
  expect(screen.getByText('No artwork')).toBeVisible();

  setImageId('second-image');
  expect(screen.getByAltText('Artwork')).toHaveAttribute('src', `${PROXY_BASE}/image/second-image`);

  dispose();
  root.remove();
});

test('reacts when the proxy becomes available', () => {
  resetImageProxyBase();
  const { dispose, root } = renderImage(() => 'signed-image');

  expect(screen.queryByAltText('Artwork')).toBeNull();
  expect(screen.getByText('No artwork')).toBeVisible();

  setImageProxyBase(PROXY_BASE);
  expect(screen.getByAltText('Artwork')).toHaveAttribute('src', `${PROXY_BASE}/image/signed-image`);

  dispose();
  root.remove();
});
