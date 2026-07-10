import { attachDevtoolsOverlay } from '@solid-devtools/overlay';
import { render } from 'solid-js/web';

import './index.css';
import App from './App';

attachDevtoolsOverlay();

const root = document.querySelector('#root');
if (root) {
  render(() => <App />, root);
}
