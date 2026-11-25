import { useState } from "react";
import type { FormEvent } from "react";

interface NewTaskFormProps {
  onCreate: (title: string) => Promise<boolean>;
}

export function NewTaskForm({ onCreate }: NewTaskFormProps) {
  const [title, setTitle] = useState("");
  const [pending, setPending] = useState(false);
  const trimmed = title.trim();
  const canSubmit = trimmed.length > 0 && !pending;

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }
    setPending(true);
    const created = await onCreate(trimmed);
    setPending(false);
    if (created) {
      setTitle("");
    }
  }

  return (
    <form className="new-task" onSubmit={handleSubmit}>
      <input
        aria-label="New task title"
        placeholder="What needs doing?"
        value={title}
        maxLength={200}
        onChange={(event) => setTitle(event.target.value)}
      />
      <button type="submit" disabled={!canSubmit}>
        Add task
      </button>
    </form>
  );
}
