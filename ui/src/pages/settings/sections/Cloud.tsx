import { useQuery } from "@tanstack/react-query";
import { ErrorAlert } from "../../../components/ErrorAlert";
import { useI18n } from "../../../i18n/useI18n";
import { cloudErrorKey } from "../../../lib/cloud";
import { queries } from "../../../lib/queries";
import { DeviceNameRow } from "./cloud/DeviceNameRow";
import { FolderField } from "./cloud/FolderField";
import { SpaceBlock } from "./cloud/SpaceBlock";

export function Cloud() {
  const { t } = useI18n();
  const { data: overview } = useQuery({ ...queries.cloudOverview(), refetchOnMount: "always" });

  if (!overview) return null;
  const workerError = overview.lastError
    ? t(cloudErrorKey(overview.lastError) ?? "cloud.status.failedDetail")
    : null;

  return (
    <>
      <ErrorAlert message={workerError} />
      <FolderField overview={overview} />
      <DeviceNameRow current={overview.deviceName} />
      {overview.folder && !overview.folderMissing && <SpaceBlock overview={overview} />}
    </>
  );
}
