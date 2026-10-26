import type { ReactNode } from "react";
import type { Key } from "../i18n/dictionary";
import type { Locale, Settings } from "./domain";

export type Vars = Record<string, string | number>;
export type Translate = (key: Key, vars?: Vars) => string;

export interface I18nValue {
  locale: Locale;
  t: Translate;
  refresh: () => void;
}

export interface ChildrenProps {
  children: ReactNode;
}

export interface MessageProps {
  message: string | null;
}

export type UpdateSetting = <K extends keyof Settings>(key: K, value: Settings[K]) => Promise<void>;

export interface SettingsState {
  settings: Settings | null;
  error: string | null;
  saved: boolean;
  revision: number;
  update: UpdateSetting;
}

export interface SectionProps {
  settings: Settings;
  revision: number;
  update: UpdateSetting;
}
