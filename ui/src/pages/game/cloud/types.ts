import type { Run } from "../../../hooks/useCloudAction";
import type { CloudGameView } from "../../../types/domain";

export interface CloudPanelProps {
  appId: number;
  view: CloudGameView;
  busy: boolean;
  run: Run;
}
