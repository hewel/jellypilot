import type { SolidFormExtendedApi } from '@tanstack/solid-form'
import { type Accessor, createContext, type Resource, useContext } from 'solid-js'
import type { ConnectionState } from '../../bindings'

export type SettingsFormValues = {
  deviceName: string
  mpvPath: string
  mpvArgs: string
  keybindNext: string
  keybindPrev: string
}

export type SettingsForm = SolidFormExtendedApi<
  SettingsFormValues,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined
>

export interface SettingsContextValue {
  form: SettingsForm
  connectionState: Resource<ConnectionState>
  mpvConnected: Resource<boolean>
  detectingMpv: Accessor<boolean>
  setDetectingMpv: (v: boolean) => void
  saveMessage: Accessor<{ type: 'success' | 'error'; text: string } | null>
  disconnecting: Accessor<boolean>
  clearingSession: Accessor<boolean>
  handleDisconnect: () => Promise<void>
  handleClearSession: () => Promise<void>
  handleDetectMpv: () => Promise<void>
}

const SettingsContext = createContext<SettingsContextValue>()

export const useSettings = () => {
  const context = useContext(SettingsContext)
  if (!context) {
    throw new Error('useSettings must be used within a SettingsProvider')
  }
  return context
}

export { SettingsContext }
