import { Icon } from "../../../components/Icon";
import { Button } from "../../../components/ui/button";
import { api } from "../../../lib/commands";
import type { UserFolder } from "../../../types/domain";

interface FolderLinkProps {
  folder: UserFolder;
  label: string;
}

export function FolderLink({ folder, label }: FolderLinkProps) {
  return (
    <Button
      variant="link"
      size="sm"
      className="w-max"
      onClick={() => {
        api.openUserFolder(folder).catch(() => undefined);
      }}
    >
      <Icon name="folder-open" size={16} />
      {label}
    </Button>
  );
}
