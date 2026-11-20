import { getSetting, setSetting } from "./settings.js";

for (const key of ["ocr", "dev"]) {
  const box = document.getElementById(key);
  box.checked = getSetting(key);
  box.addEventListener("change", () => setSetting(key, box.checked));
}
