import { E2E_ROOT } from '../constants';
import { removeOwnedDirectory } from '../support/ownership';

export const cleanE2e = removeOwnedDirectory(E2E_ROOT);
