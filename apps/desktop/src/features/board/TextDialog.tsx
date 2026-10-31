import { useState } from "react";
import { Dialog } from "../../shared/ui/Dialog";

interface TextDialogProps {
  title: string;
  initial: string;
  onDone: (text: string | null) => void;
}

/** One line of text. Enter confirms and Esc cancels. */
export function TextDialog({ title, initial, onDone }: TextDialogProps) {
  const [text, setText] = useState(initial);

  return (
    <Dialog title={title} onCancel={() => onDone(null)}>
      <input
        className="field"
        aria-label={title}
        value={text}
        autoFocus
        onFocus={(event) => event.target.select()}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && onDone(text)}
      />
    </Dialog>
  );
}
