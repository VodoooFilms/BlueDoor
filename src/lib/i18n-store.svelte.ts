import { COPY, getInitialLang, saveLang, type Copy, type Lang } from "./i18n";

const initialLang = getInitialLang();
let lang: Lang = $state(initialLang);
document.documentElement.lang = initialLang;

export function getCopy(): Copy {
  return COPY[lang];
}

export function getLang(): Lang {
  return lang;
}

export function setLang(next: Lang) {
  lang = next;
  saveLang(next);
  document.documentElement.lang = next;
}

export function toggleLang() {
  setLang(lang === "es" ? "en" : "es");
}
