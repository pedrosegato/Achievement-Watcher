import type { Locale } from "../types/domain";

const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 3600;
const WHOLE_HOURS_FROM = 100;
const SMALL_PERCENT = 1;
const BYTE_STEP = 1024;
const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;
const ONE_DECIMAL_BELOW = 10;

const toDate = (seconds: number) => new Date(seconds * 1000);

const isPortuguese = (locale: Locale) => locale.toLowerCase().startsWith("pt");

const DAY: Intl.DateTimeFormatOptions = { day: "numeric", month: "short", year: "numeric" };
const TIME: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit" };

const numberFormats = new Map<string, Intl.NumberFormat>();
const dateFormats = new Map<string, Intl.DateTimeFormat>();

function numberFormat(locale: Locale, min: number, max: number): Intl.NumberFormat {
  const key = `${locale}|${min}|${max}`;
  let format = numberFormats.get(key);
  if (!format) {
    format = new Intl.NumberFormat(locale, {
      minimumFractionDigits: min,
      maximumFractionDigits: max,
    });
    numberFormats.set(key, format);
  }
  return format;
}

function dateFormat(locale: Locale, options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const key = `${locale}|${options === DAY ? "day" : "time"}`;
  let format = dateFormats.get(key);
  if (!format) {
    format = new Intl.DateTimeFormat(locale, options);
    dateFormats.set(key, format);
  }
  return format;
}

function formatNumber(locale: Locale, value: number, min: number, max: number): string {
  return numberFormat(locale, min, max).format(value);
}

function dateParts(seconds: number, locale: Locale) {
  const parts: Partial<Record<Intl.DateTimeFormatPartTypes, string>> = {};
  for (const part of dateFormat(locale, DAY).formatToParts(toDate(seconds))) {
    parts[part.type] = part.value;
  }
  return parts;
}

export function formatPercent(value: number, locale: Locale): string {
  return `${formatNumber(locale, value, 0, value < SMALL_PERCENT ? 2 : 1)}%`;
}

export function formatBytes(bytes: number, locale: Locale): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= BYTE_STEP && unit < BYTE_UNITS.length - 1) {
    value /= BYTE_STEP;
    unit += 1;
  }
  const decimals = unit > 0 && value < ONE_DECIMAL_BELOW ? 1 : 0;
  return `${formatNumber(locale, value, 0, decimals)} ${BYTE_UNITS[unit]}`;
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
  const { day, month, year } = dateParts(seconds, locale);
  const shortMonth = (month ?? "").replace(".", "");
  return isPortuguese(locale) ? `${day} ${shortMonth} ${year}` : `${shortMonth} ${day}, ${year}`;
}

export function formatTime(seconds: number, locale: Locale): string {
  return dateFormat(locale, TIME).format(toDate(seconds)).replace(/\s/g, " ");
}

export function formatDateTime(seconds: number, locale: Locale): string {
  return `${formatDay(seconds, locale)}, ${formatTime(seconds, locale)}`;
}

const MS_PER_DAY = 86_400_000;
const relativeDays = new Map<string, Intl.RelativeTimeFormat>();

function startOfDay(seconds: number): number {
  const date = toDate(seconds);
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

export function formatWhen(seconds: number, locale: Locale, now: number): string {
  const days = Math.round((startOfDay(now) - startOfDay(seconds)) / MS_PER_DAY);
  if (days !== 0 && days !== 1) return formatDateTime(seconds, locale);
  let relative = relativeDays.get(locale);
  if (!relative) {
    relative = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
    relativeDays.set(locale, relative);
  }
  const word = relative.format(-days, "day");
  return `${word.charAt(0).toLocaleUpperCase(locale)}${word.slice(1)}, ${formatTime(seconds, locale)}`;
}
