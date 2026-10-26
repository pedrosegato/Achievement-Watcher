import { createContext } from "react";
import type { Settings } from "../types/domain";

export const AppSettingsContext = createContext<Settings | null>(null);
