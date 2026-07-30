// @rstest-environment jsdom
import { afterEach, beforeEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen } from '@testing-library/dom';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import { LibraryImage } from '../src/components/library/LibraryImage';
import { resetImageProxyBase, setImageProxyBase } from '../src/utils/imageSource';

const PROXY_BASE = 'http://127.0.0.1:43127';

beforeEach(() => setImageProxyBase(PROXY_BASE));
afterEach(() => {
  rstest.restoreAllMocks();
  resetImageProxyBase();
  document.body.innerHTML = '';
});

function mockRejectAvif() {
  return rstest.spyOn(commands, 'imageRejectAvif').mockResolvedValue({ status: 'ok', data: null });
}

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

test('first load error rejects the AVIF once and retries with a cache bust', () => {
  const reject = mockRejectAvif();
  const { dispose, root } = renderImage(() => 'signed-image');

  fireEvent.error(screen.getByAltText('Artwork'));

  expect(reject).toHaveBeenCalledTimes(1);
  expect(reject).toHaveBeenCalledWith('signed-image');
  expect(screen.getByAltText('Artwork')).toHaveAttribute(
    'src',
    `${PROXY_BASE}/image/signed-image?retry=1`,
  );

  dispose();
  root.remove();
});

test('second load error gives up and renders the fallback without rejecting again', () => {
  const reject = mockRejectAvif();
  const { dispose, root } = renderImage(() => 'broken-image');

  fireEvent.error(screen.getByAltText('Artwork'));
  fireEvent.error(screen.getByAltText('Artwork'));

  expect(reject).toHaveBeenCalledTimes(1);
  expect(screen.queryByAltText('Artwork')).toBeNull();
  expect(screen.getByText('No artwork')).toBeVisible();

  dispose();
  root.remove();
});

test('changing the image reference resets the retry attempt', () => {
  const reject = mockRejectAvif();
  const [imageId, setImageId] = createSignal<string | null>('first-image');
  const { dispose, root } = renderImage(imageId);

  fireEvent.error(screen.getByAltText('Artwork'));
  expect(screen.getByAltText('Artwork')).toHaveAttribute(
    'src',
    `${PROXY_BASE}/image/first-image?retry=1`,
  );

  setImageId('second-image');
  expect(screen.getByAltText('Artwork')).toHaveAttribute('src', `${PROXY_BASE}/image/second-image`);

  fireEvent.error(screen.getByAltText('Artwork'));
  expect(reject).toHaveBeenCalledTimes(2);
  expect(reject).toHaveBeenLastCalledWith('second-image');

  dispose();
  root.remove();
});

test('renders the fallback while the proxy is unavailable', () => {
  resetImageProxyBase();
  const { dispose, root } = renderImage(() => 'signed-image');

  expect(screen.queryByAltText('Artwork')).toBeNull();
  expect(screen.getByText('No artwork')).toBeVisible();

  dispose();
  root.remove();
});
