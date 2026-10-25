import type { Locale } from "../types/domain";

const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 3600;
const WHOLE_HOURS_FROM = 100;
const SMALL_PERCENT = 1;

const toDate = (seconds: number) => new Date(seconds * 1000);

const isPortuguese = (locale: Locale) => locale.toLowerCase().startsWith("pt");

function formatNumber(locale: Locale, value: number, min: number, max: number): string {
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: min,
    maximumFractionDigits: max,
  }).format(value);
}

function dateParts(seconds: number, locale: Locale, options: Intl.DateTimeFormatOptions) {
  const parts: Partial<Record<Intl.DateTimeFormatPartTypes, string>> = {};
  for (const part of new Intl.DateTimeFormat(locale, options).formatToParts(toDate(seconds))) {
    parts[part.type] = part.value;
  }
  return parts;
}

export function formatPercent(value: number, locale: Locale): string {
  return `${formatNumber(locale, value, 0, value < SMALL_PERCENT ? 2 : 1)}%`;
}

export function formatHours(seconds: number, locale: Locale): string {
  if (seconds < SECONDS_PER_HOUR) {
    return `${Math.max(1, Math.round(seconds / SECONDS_PER_MINUTE))} min`;
  }
  const hours = seconds / SECONDS_PER_HOUR;
  return hours >= WHOLE_HOURS_FROM
    ? `${formatNumber(locale, Math.round(hours), 0, 0)} h`
    : `${formatNumber(locale, Math.floor(hours * 10) / 10, 1, 1)} h`;
}

export function formatDay(seconds: number, locale: Locale): string {
  const { day, month, year } = dateParts(seconds, locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
  const shortMonth = (month ?? "").replace(".", "");
  return isPortuguese(locale) ? `${day} ${shortMonth} ${year}` : `${shortMonth} ${day}, ${year}`;
}

export function formatTime(seconds: number, locale: Locale): string {
  return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" })
    .format(toDate(seconds))
    .replace(/\s/g, " ");
}

export function formatDateTime(seconds: number, locale: Locale): string {
  return `${formatDay(seconds, locale)}, ${formatTime(seconds, locale)}`;
}
