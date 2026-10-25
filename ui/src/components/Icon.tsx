import {
  Add01Icon,
  ArrowDown01Icon,
  ArrowLeft01Icon,
  Calendar03Icon,
  Camera01Icon,
  Cancel01Icon,
  ChampionIcon,
  Clock01Icon,
  ComputerIcon,
  Copy01Icon,
  Delete02Icon,
  Download04Icon,
  Folder01Icon,
  FolderOpenIcon,
  Image01Icon,
  InformationCircleIcon,
  KeyboardIcon,
  MinusSignIcon,
  Moon02Icon,
  Notification01Icon,
  PlayIcon,
  Refresh01Icon,
  Search01Icon,
  Settings01Icon,
  SquareIcon,
  SquareLock02Icon,
  Sun03Icon,
  Tick02Icon,
  Video01Icon,
  ViewIcon,
  ViewOffSlashIcon,
  VolumeHighIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

const ICONS = {
  settings: Settings01Icon,
  search: Search01Icon,
  sync: Refresh01Icon,
  trophy: ChampionIcon,
  clock: Clock01Icon,
  calendar: Calendar03Icon,
  eye: ViewIcon,
  "eye-off": ViewOffSlashIcon,
  "caret-down": ArrowDown01Icon,
  back: ArrowLeft01Icon,
  close: Cancel01Icon,
  lock: SquareLock02Icon,
  sun: Sun03Icon,
  moon: Moon02Icon,
  monitor: ComputerIcon,
  folder: Folder01Icon,
  "folder-open": FolderOpenIcon,
  bell: Notification01Icon,
  volume: VolumeHighIcon,
  camera: Camera01Icon,
  image: Image01Icon,
  video: Video01Icon,
  info: InformationCircleIcon,
  add: Add01Icon,
  delete: Delete02Icon,
  play: PlayIcon,
  download: Download04Icon,
  keyboard: KeyboardIcon,
  check: Tick02Icon,
  minimize: MinusSignIcon,
  maximize: SquareIcon,
  restore: Copy01Icon,
} as const;

export type IconName = keyof typeof ICONS;

interface IconProps {
  name: IconName;
  size?: number;
  className?: string;
}

const STROKE = 1.8;

export function Icon({ name, size = 18, className }: IconProps) {
  return (
    <HugeiconsIcon
      icon={ICONS[name]}
      size={size}
      strokeWidth={STROKE}
      className={className}
      aria-hidden="true"
    />
  );
}
