/* @refresh reload */
import { render } from 'solid-js/web'
import '@jellypilot/ui/reset.css'
import { App } from './App'

const root = document.getElementById('root')
if (!root) throw new Error('catalog root missing')
render(() => <App />, root)
