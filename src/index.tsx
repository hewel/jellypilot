import { attachDevtoolsOverlay } from '@solid-devtools/overlay';
import '@fontsource-variable/inter';
import '@fontsource-variable/space-grotesk';
import { render } from 'solid-js/web';

import './styles/vars.css';
import './index.css';
import App from './App';
import { PandaPrototypePage } from './routes/panda-prototype';

attachDevtoolsOverlay();

const root = document.querySelector('#root');
if (root) {
  const showPandaPrototype = new URLSearchParams(window.location.search).has('panda-prototype');
  render(() => (showPandaPrototype ? <PandaPrototypePage /> : <App />), root);
}
