const KEY = "rustapi:key";

export function getKey(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export function setKey(key: string): void {
  try {
    localStorage.setItem(KEY, key);
  } catch {
    // ignore quota / privacy-mode errors
  }
}

export function clearKey(): void {
  try {
    localStorage.removeItem(KEY);
  } catch {
    // ignore
  }
}
