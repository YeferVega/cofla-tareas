import { useCallback, useEffect, useState } from "react";
import { ApiError, createTask, deleteTask, fetchTasks, setTaskDone } from "./api/tasks";
import type { Task } from "./api/tasks";
import { ErrorBanner } from "./components/ErrorBanner";
import { NewTaskForm } from "./components/NewTaskForm";
import { TaskList } from "./components/TaskList";

function messageFrom(error: unknown): string {
  if (error instanceof ApiError) {
    return error.message;
  }
  return "Something went wrong. Please try again.";
}

export function App() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    fetchTasks()
      .then((loaded) => {
        if (active) {
          setTasks(loaded);
          setError("");
        }
      })
      .catch((error: unknown) => {
        if (active) {
          setError(messageFrom(error));
        }
      })
      .finally(() => {
        if (active) {
          setLoading(false);
        }
      });
    return () => {
      active = false;
    };
  }, []);

  const handleCreate = useCallback(async (title: string): Promise<boolean> => {
    try {
      const created = await createTask(title);
      setTasks((previous) => [...previous, created]);
      setError("");
      return true;
    } catch (error) {
      setError(messageFrom(error));
      return false;
    }
  }, []);

  const handleToggle = useCallback(async (task: Task): Promise<void> => {
    try {
      const updated = await setTaskDone(task.id, !task.done);
      setTasks((previous) =>
        previous.map((item) => (item.id === updated.id ? updated : item)),
      );
      setError("");
    } catch (error) {
      setError(messageFrom(error));
    }
  }, []);

  const handleDelete = useCallback(async (task: Task): Promise<void> => {
    try {
      await deleteTask(task.id);
      setTasks((previous) => previous.filter((item) => item.id !== task.id));
      setError("");
    } catch (error) {
      setError(messageFrom(error));
    }
  }, []);

  return (
    <main className="app">
      <h1>Cofla Tareas</h1>
      <p className="subtitle">A small shared task list.</p>
      {error !== "" && <ErrorBanner message={error} />}
      <NewTaskForm onCreate={handleCreate} />
      {loading ? (
        <p className="loading">Loading tasks...</p>
      ) : (
        <TaskList tasks={tasks} onToggle={handleToggle} onDelete={handleDelete} />
      )}
    </main>
  );
}
