// Settings live in localStorage, which the popup and the options page share
// (same moz-extension:// origin). No extension permission needed.
const DEFAULTS = { ocr: true, dev: false };

export function getSetting(key) {
  try {
    const v = localStorage.getItem(`setting:${key}`);
    return v === null ? DEFAULTS[key] : v === "1";
  } catch {
    return DEFAULTS[key];
  }
}

export function setSetting(key, value) {
  try { localStorage.setItem(`setting:${key}`, value ? "1" : "0"); } catch {}
}
