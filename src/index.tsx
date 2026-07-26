import { attachDevtoolsOverlay } from '@solid-devtools/overlay';
import '@fontsource-variable/archivo';
import '@fontsource-variable/inter';
import '@fontsource-variable/jetbrains-mono';
import '@fontsource-variable/space-grotesk';

import './index.css';
import { mountApplication } from './mountApplication';

attachDevtoolsOverlay();

void mountApplication();
