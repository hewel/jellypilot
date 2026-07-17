import { render } from 'solid-js/web';

import App from './App';

let mounted = false;

export function mountApplication(): void {
  if (mounted) return;

  const root = document.querySelector('#root');
  if (!root) return;

  mounted = true;
  render(() => <App />, root);
}
