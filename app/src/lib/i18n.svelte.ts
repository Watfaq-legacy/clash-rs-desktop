// Two UI languages. Strings stay inline: t("已连接", "Connected").
// The backend resolves the user's choice against the OS locale and sends
// it with every view; until the first view arrives, guess from the webview.

let english = $state(!navigator.language.toLowerCase().startsWith("zh"));

export function setEnglish(value: boolean) {
  english = value;
  document.documentElement.lang = value ? "en" : "zh-CN";
}

export function isEnglish(): boolean {
  return english;
}

export function t(zh: string, en: string): string {
  return english ? en : zh;
}
