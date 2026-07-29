import { expect, test } from '@rstest/core';

import { imageSource, resetImageProxyBase, setImageProxyBase } from '../src/utils/imageSource';

test('imageSource creates a localhost proxy URL from a signed image id', () => {
  setImageProxyBase('http://127.0.0.1:43127');

  expect(imageSource('signed/image id')).toBe('http://127.0.0.1:43127/image/signed%2Fimage%20id');
  resetImageProxyBase();
});

test('imageSource is empty while the image proxy is unavailable', () => {
  resetImageProxyBase();

  expect(imageSource('signed-image-id')).toBe('');
});
