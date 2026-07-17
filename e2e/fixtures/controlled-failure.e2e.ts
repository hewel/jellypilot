import { mountNativeApp } from '../support/native-app';

describe('controlled harness failure', () => {
  it('retains scrubbed evidence without entering default discovery', async () => {
    await mountNativeApp();
    throw new Error('JELLYPILOT_CONTROLLED_FAILURE');
  });
});
