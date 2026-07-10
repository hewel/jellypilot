import { attachDevtoolsOverlay } from '@solid-devtools/overlay';
import '@wdio/tauri-plugin';
import { render } from 'solid-js/web';

import './index.css';
import App from './App';

attachDevtoolsOverlay();

declare global {
  interface Window {
    __JELLYPILOT_MOUNT__?: () => void;
  }
}

window.__JELLYPILOT_MOUNT__ = () => {
  const root = document.querySelector('#root');
  if (root) {
    render(() => <App />, root);
  }
};
