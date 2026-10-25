import { DEFAULT_SOUND_PACK } from "../constants";
import type { SoundPack } from "../types/domain";
import type { Translate } from "../types/ui";

export function packName(pack: SoundPack, t: Translate): string {
  if (!pack.builtin) return t("settings.customPack", { name: pack.name });
  return pack.id === DEFAULT_SOUND_PACK ? t("settings.defaultPack") : pack.name;
}

export function selectedPackId(packs: SoundPack[], wanted: string): string {
  const id = wanted.toLowerCase();
  return packs.find((pack) => pack.id.toLowerCase() === id)?.id ?? DEFAULT_SOUND_PACK;
}
