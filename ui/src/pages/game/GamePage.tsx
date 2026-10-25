import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { ErrorAlert } from "../../components/ErrorAlert";
import { errorMessage } from "../../lib/commands";
import { queries } from "../../lib/queries";
import type { AchievementFilter } from "../../types/domain";
import { AchievementsPanel } from "./AchievementsPanel";
import { ActivityTimeline } from "./ActivityTimeline";
import { AchievementsDialog } from "./dialog/AchievementsDialog";
import { FactsStrip } from "./FactsStrip";
import { GameHero } from "./GameHero";
import { GameSkeleton } from "./GameSkeleton";

interface GamePageProps {
  appId: number;
  onBack: () => void;
}

export function GamePage({ appId, onBack }: GamePageProps) {
  const { data: detail, error } = useQuery(queries.game(appId));
  const [dialogOpen, setDialogOpen] = useState(false);
  const [filter, setFilter] = useState<AchievementFilter>("all");
  const openDialog = (next: AchievementFilter) => {
    setFilter(next);
    setDialogOpen(true);
  };

  if (!detail) {
    return error ? (
      <div className="px-5 pt-5">
        <ErrorAlert message={errorMessage(error)} />
      </div>
    ) : (
      <GameSkeleton />
    );
  }

  return (
    <div>
      <GameHero detail={detail} onBack={onBack} />
      <FactsStrip detail={detail} />
      <div className="game-cols mx-auto max-w-[1440px] px-5 pt-5 pb-10">
        <ActivityTimeline detail={detail} />
        <AchievementsPanel detail={detail} onOpenDialog={openDialog} />
      </div>
      <AchievementsDialog
        detail={detail}
        open={dialogOpen}
        filter={filter}
        onFilter={setFilter}
        onOpenChange={setDialogOpen}
      />
    </div>
  );
}
