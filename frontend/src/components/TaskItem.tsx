import type { Task } from "../api/tasks";

interface TaskItemProps {
  task: Task;
  onToggle: (task: Task) => Promise<void>;
  onDelete: (task: Task) => Promise<void>;
}

export function TaskItem({ task, onToggle, onDelete }: TaskItemProps) {
  const className = task.done ? "task done" : "task";
  return (
    <li className={className}>
      <label className="task-title">
        <input
          type="checkbox"
          checked={task.done}
          onChange={() => {
            void onToggle(task);
          }}
          aria-label={`Toggle ${task.title}`}
        />
        <span>{task.title}</span>
      </label>
      <button
        type="button"
        className="delete"
        aria-label={`Delete ${task.title}`}
        onClick={() => {
          void onDelete(task);
        }}
      >
        Delete
      </button>
    </li>
  );
}
