import { expect, test } from '@rstest/core';

import { addServiceDialogOverflowLayout } from '../src/components/OperationsConsole.styles';

test('add service dialog keeps over-height content reachable from the scroll origin', () => {
  expect(addServiceDialogOverflowLayout).toEqual({
    contentMarginBlock: 'auto',
    positionerAlignItems: 'flex-start',
    positionerOverflowY: 'auto',
  });
});
