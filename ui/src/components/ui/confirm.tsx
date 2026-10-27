import { Button } from "./button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./dialog";

interface ConfirmProps {
  title: string;
  body: string;
  confirm: string;
  secondary: string;
  onConfirm: () => void;
  onSecondary: () => void;
}

export function Confirm({ title, body, confirm, secondary, onConfirm, onSecondary }: ConfirmProps) {
  return (
    <Dialog open onOpenChange={(open) => !open && onSecondary()}>
      <DialogContent className="w-[min(92vw,460px)] px-6 pt-5 pb-5">
        <DialogTitle className="font-bold text-[18px] leading-snug">{title}</DialogTitle>
        <DialogDescription className="mt-2 break-words text-[13.5px] text-muted leading-relaxed">
          {body}
        </DialogDescription>
        <div className="mt-5 flex justify-end gap-2">
          <Button onClick={onSecondary}>{secondary}</Button>
          <Button variant="primary" onClick={onConfirm}>
            {confirm}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
