import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { EVENTS } from "../constants";
import { useScan } from "../hooks/useScan";
import { useTauriEvent } from "../hooks/useTauriEvent";
import { cn } from "../lib/cn";
import { invalidateGames } from "../lib/queries";
import { GamePage } from "../pages/game/GamePage";
import { LibraryPage } from "../pages/library/LibraryPage";
import { SettingsDialog } from "../pages/settings/SettingsDialog";
import { Topbar } from "./Topbar";

export function App() {
  const [game, setGame] = useState<number | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsTopic, setSettingsTopic] = useState("general");
  const openSettings = (topic: string) => {
    setSettingsTopic(topic);
    setSettingsOpen(true);
  };
  const [query, setQuery] = useState("");
  const { scanning, start } = useScan();
  const queryClient = useQueryClient();
  useTauriEvent(EVENTS.gamesChanged, () => void invalidateGames(queryClient));

  return (
    <div className="flex h-screen flex-col">
      <Topbar settingsOpen={settingsOpen} onSettings={() => openSettings("general")} />
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)] grid-rows-[minmax(0,1fr)]">
        <main
          inert={game !== null}
          className={cn(
            "col-start-1 row-start-1 overflow-y-auto",
            game !== null && "invisible [content-visibility:hidden]",
          )}
        >
          <div className="mx-auto max-w-[1440px] px-5 pt-5 pb-10">
            <LibraryPage
              query={query}
              onSearch={setQuery}
              scanning={scanning}
              onRescan={start}
              onOpen={setGame}
            />
          </div>
        </main>
        {game !== null && (
          <main key={game} className="col-start-1 row-start-1 overflow-y-auto">
            <GamePage appId={game} onBack={() => setGame(null)} onOpenSettings={openSettings} />
          </main>
        )}
      </div>
      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} topic={settingsTopic} />
    </div>
  );
}
