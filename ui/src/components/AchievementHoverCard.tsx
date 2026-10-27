import type { ReactNode } from "react";
import { useMotionMs } from "../lib/motion";
import type { AchievementRow, RecentIcon } from "../types/domain";
import { AchievementHoverDetails } from "./AchievementHoverDetails";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "./ui/hover-card";

interface AchievementHoverCardProps {
  achievement: RecentIcon | AchievementRow;
  rare?: boolean;
  children: ReactNode;
}

export function AchievementHoverCard({
  achievement,
  rare = false,
  children,
}: AchievementHoverCardProps) {
  const openDelay = useMotionMs("--hover-delay");

  return (
    <HoverCard openDelay={openDelay} closeDelay={0}>
      <HoverCardTrigger asChild>{children}</HoverCardTrigger>
      <HoverCardContent side="top" align="center" className="flex w-[300px] flex-col gap-2.5">
        <AchievementHoverDetails achievement={achievement} rare={rare} />
      </HoverCardContent>
    </HoverCard>
  );
}
