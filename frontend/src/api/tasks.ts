export interface Task {
  id: number;
  title: string;
  done: boolean;
}

export class ApiError extends Error {}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, {
    method,
    headers: body === undefined ? {} : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const message =
      typeof data === "object" && data !== null && "error" in data
        ? String((data as { error: unknown }).error)
        : "Request failed";
    throw new ApiError(message);
  }
  return data as T;
}

export function fetchTasks(): Promise<Task[]> {
  return request<Task[]>("GET", "/api/tasks");
}

export function createTask(title: string): Promise<Task> {
  return request<Task>("POST", "/api/tasks", { title });
}

export function setTaskDone(id: number, done: boolean): Promise<Task> {
  return request<Task>("PATCH", `/api/tasks/${id}`, { done });
}

export function deleteTask(id: number): Promise<{ ok: true }> {
  return request<{ ok: true }>("DELETE", `/api/tasks/${id}`);
}
