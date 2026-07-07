import { attachDevtoolsOverlay } from '@solid-devtools/overlay';
import '@fontsource-variable/figtree';
import { render } from 'solid-js/web';

import './styles/vars.css';
import './index.css';
import App from './App';

attachDevtoolsOverlay();

const root = document.querySelector('#root');
if (root) {
  render(() => <App />, root);
}
