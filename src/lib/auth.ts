import { commands, type SavedSession } from '../bindings';

const SESSION_STORAGE_KEY = 'jmsr_auth_session';

export function loadSavedSession(): SavedSession | null {
  try {
    const saved = localStorage.getItem(SESSION_STORAGE_KEY);
    if (saved) {
      return JSON.parse(saved) as SavedSession;
    }
  } catch {
    // Ignore parse errors
  }
  return null;
}

export function saveSession(session: SavedSession): void {
  localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(session));
}

export function clearSavedSession(): void {
  localStorage.removeItem(SESSION_STORAGE_KEY);
}

// Check if user is authenticated (connected to Jellyfin)
export async function checkAuth(): Promise<boolean> {
  try {
    const isConnected = await commands.jellyfinIsConnected();
    if (isConnected) {
      return true;
    }

    // Try to restore saved session
    const savedSession = loadSavedSession();
    if (savedSession) {
      const result = await commands.jellyfinRestoreSession(savedSession);
      if (result.status === 'ok') {
        return true;
      }
      // Session restoration failed - clear invalid session
      clearSavedSession();
    }
    return false;
  } catch {
    return false;
  }
}
