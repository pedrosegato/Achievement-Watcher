import type { ReactNode } from "react";
import { ErrorAlert } from "../../components/ErrorAlert";
import { Icon, type IconName } from "../../components/Icon";
import { Dialog, DialogClose, DialogContent, DialogTitle } from "../../components/ui/dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../../components/ui/tabs";
import { useSettings } from "../../hooks/useSettings";
import type { Key } from "../../i18n/dictionary";
import { useI18n } from "../../i18n/useI18n";
import type { SectionProps } from "../../types/ui";
import { About } from "./sections/About";
import { Captures } from "./sections/Captures";
import { Cloud } from "./sections/Cloud";
import { Games } from "./sections/Games";
import { General } from "./sections/General";
import { Notifications } from "./sections/Notifications";
import { Sound } from "./sections/Sound";

interface Topic {
  id: string;
  label: Key;
  icon: IconName;
  Section: (props: SectionProps) => ReactNode;
}

const TOPICS: Topic[] = [
  { id: "general", label: "settings.nav.general", icon: "settings", Section: General },
  {
    id: "notifications",
    label: "settings.nav.notifications",
    icon: "bell",
    Section: Notifications,
  },
  { id: "sound", label: "settings.nav.sound", icon: "volume", Section: Sound },
  { id: "games", label: "settings.nav.games", icon: "folder", Section: Games },
  { id: "cloud", label: "settings.nav.cloud", icon: "cloud", Section: Cloud },
  { id: "captures", label: "settings.nav.captures", icon: "camera", Section: Captures },
  { id: "about", label: "settings.nav.about", icon: "info", Section: About },
];

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  topic?: string;
}

export function SettingsDialog({ open, onOpenChange, topic = "general" }: SettingsDialogProps) {
  const { t } = useI18n();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        aria-describedby={undefined}
        className="h-[min(720px,calc(100vh-var(--titlebar)-48px))] max-h-none w-[min(980px,calc(100vw-48px))] bg-bg"
      >
        <SettingsBody topic={topic} />
        <DialogClose asChild>
          <button
            type="button"
            aria-label={t("window.close")}
            className="absolute top-3 right-3 grid size-8 cursor-pointer place-items-center rounded-card bg-bg text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) hover:bg-surface-2 hover:text-text"
          >
            <Icon name="close" size={16} />
          </button>
        </DialogClose>
      </DialogContent>
    </Dialog>
  );
}

function SettingsBody({ topic }: { topic: string }) {
  const { t } = useI18n();
  const { settings, error, revision, update } = useSettings();

  return (
    <Tabs defaultValue={topic} orientation="vertical" className="flex min-h-0 flex-1">
      <div className="flex w-[210px] shrink-0 flex-col gap-3 border-line border-r bg-surface px-3 py-4">
        <DialogTitle className="px-2.5 font-bold text-[18px]">{t("nav.settings")}</DialogTitle>
        <TabsList aria-label={t("nav.settings")} className="flex-col">
          {TOPICS.map((topic) => (
            <TabsTrigger key={topic.id} value={topic.id}>
              <Icon name={topic.icon} size={18} />
              {t(topic.label)}
            </TabsTrigger>
          ))}
        </TabsList>
      </div>
      <div className="min-w-0 flex-1 overflow-y-auto px-7 pt-5 pb-7">
        <ErrorAlert message={error} />
        {settings &&
          TOPICS.map(({ id, label, Section }) => (
            <TabsContent key={id} value={id} className="settings-section outline-none">
              <h2 className="mb-1.5 pr-10 font-bold text-[20px]">{t(label)}</h2>
              <Section settings={settings} revision={revision} update={update} />
            </TabsContent>
          ))}
      </div>
    </Tabs>
  );
}
