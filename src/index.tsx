import { attachDevtoolsOverlay } from '@solid-devtools/overlay';
import '@fontsource-variable/inter';
import '@fontsource-variable/space-grotesk';

import './index.css';
import { mountApplication } from './mountApplication';

attachDevtoolsOverlay();

mountApplication();
