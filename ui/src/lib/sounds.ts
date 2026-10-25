import { DEFAULT_SOUND_PACK } from "../constants";
import type { SoundPack } from "../types/domain";
import type { Translate } from "../types/ui";

export function packName(pack: SoundPack, t: Translate): string {
  if (!pack.builtin) return t("settings.customPack", { name: pack.name });
  return pack.id === DEFAULT_SOUND_PACK ? t("settings.defaultPack") : pack.name;
}
