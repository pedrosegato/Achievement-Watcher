const STEAM_TO_BCP47: Record<string, string> = {
  english: "en",
  brazilian: "pt-BR",
  portuguese: "pt-PT",
  spanish: "es-ES",
  latam: "es-419",
  french: "fr",
  german: "de",
  italian: "it",
  russian: "ru",
  polish: "pl",
  turkish: "tr",
  ukrainian: "uk",
  japanese: "ja",
  koreana: "ko",
  schinese: "zh-Hans",
  tchinese: "zh-Hant",
};

export function steamLanguageName(steam: string, locale: string): string {
  const code = STEAM_TO_BCP47[steam];
  if (!code) return steam;
  try {
    return new Intl.DisplayNames([locale], { type: "language" }).of(code) ?? steam;
  } catch {
    return steam;
  }
}
