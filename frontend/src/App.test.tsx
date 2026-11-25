import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { Task } from "./api/tasks";

const EMPTY_LIST: Task[] = [];
const input = () => screen.getByRole("textbox", { name: "New task title" });
const submitButton = () => screen.getByRole("button", { name: "Add task" });

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

const fetchMock = vi.fn();

function queue(...responses: Response[]) {
  for (const response of responses) {
    fetchMock.mockResolvedValueOnce(response);
  }
}

function requestBody(call: number): unknown {
  const init = fetchMock.mock.calls[call][1] as RequestInit;
  return JSON.parse(String(init.body));
}

function pathOf(call: number): string {
  return String(fetchMock.mock.calls[call][0]);
}

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  fetchMock.mockReset();
});

describe("initial load", () => {
  it("fetches the task list and shows the empty state", async () => {
    queue(jsonResponse(200, EMPTY_LIST));
    render(<App />);

    await waitFor(() => expect(screen.queryByText("Loading tasks...")).not.toBeInTheDocument());
    expect(pathOf(0)).toBe("/api/tasks");
    expect(screen.getByText("No tasks yet. Add your first one.")).toBeInTheDocument();
  });

  it("renders tasks returned by the API", async () => {
    queue(
      jsonResponse(200, [
        { id: 1, title: "write spec", done: false },
        { id: 2, title: "ship docker image", done: true },
      ]),
    );
    render(<App />);

    expect(await screen.findByText("write spec")).toBeInTheDocument();
    expect(screen.getByText("ship docker image")).toBeInTheDocument();
    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes[0]).not.toBeChecked();
    expect(checkboxes[1]).toBeChecked();
  });

  it("shows an error banner when loading fails", async () => {
    queue(jsonResponse(503, { error: "Service unavailable" }));
    render(<App />);

    const banner = await screen.findByRole("alert");
    expect(banner).toHaveTextContent("Service unavailable");
  });
});

describe("task creation", () => {
  it("posts a trimmed title, appends the result, and clears the input", async () => {
    queue(jsonResponse(200, EMPTY_LIST));
    queue(jsonResponse(201, { id: 1, title: "buy milk", done: false }));
    render(<App />);

    fireEvent.change(input(), { target: { value: "  buy milk  " } });
    fireEvent.click(submitButton());

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(pathOf(1)).toBe("/api/tasks");
    expect(requestBody(1)).toEqual({ title: "buy milk" });
    expect(await screen.findByText("buy milk")).toBeInTheDocument();
    await waitFor(() => expect(input()).toHaveValue(""));
  });

  it("enforces the 200 character limit on the input", async () => {
    queue(jsonResponse(200, EMPTY_LIST));
    render(<App />);

    await screen.findByText("No tasks yet. Add your first one.");
    expect(input()).toHaveAttribute("maxLength", "200");
  });

  it("disables submit while the field is empty", async () => {
    queue(jsonResponse(200, EMPTY_LIST));
    render(<App />);

    await screen.findByText("No tasks yet. Add your first one.");
    expect(submitButton()).toBeDisabled();

    fireEvent.change(input(), { target: { value: "   " } });
    expect(submitButton()).toBeDisabled();

    fireEvent.change(input(), { target: { value: "real title" } });
    expect(submitButton()).toBeEnabled();
  });

  it("disables submit while the request is pending and re-enables after", async () => {
    queue(jsonResponse(200, EMPTY_LIST));
    let resolveCreate: (response: Response) => void = () => {};
    fetchMock.mockImplementationOnce(
      () => new Promise((resolve) => {
        resolveCreate = resolve;
      }),
    );
    render(<App />);

    await screen.findByText("No tasks yet. Add your first one.");
    fireEvent.change(input(), { target: { value: "hang" } });
    fireEvent.click(submitButton());

    await waitFor(() => expect(submitButton()).toBeDisabled());
    resolveCreate(jsonResponse(201, { id: 1, title: "hang", done: false }));

    expect(await screen.findByText("hang")).toBeInTheDocument();
    await waitFor(() => expect(input()).toHaveValue(""));
    fireEvent.change(input(), { target: { value: "next one" } });
    expect(submitButton()).toBeEnabled();
  });

  it("shows the API error message when creation fails", async () => {
    queue(jsonResponse(200, EMPTY_LIST));
    queue(
      jsonResponse(400, {
        error: "Task title must be between 1 and 200 characters",
      }),
    );
    render(<App />);

    await screen.findByText("No tasks yet. Add your first one.");
    fireEvent.change(input(), { target: { value: "still fine" } });
    fireEvent.click(submitButton());

    const banner = await screen.findByRole("alert");
    expect(banner).toHaveTextContent("Task title must be between 1 and 200 characters");
    expect(screen.queryByText("still fine")).not.toBeInTheDocument();
  });
});

describe("toggle and delete", () => {
  const firstTask: Task = { id: 1, title: "buy milk", done: false };
  const secondTask: Task = { id: 2, title: "write spec", done: false };

  it("patches the completion flag and updates the row", async () => {
    queue(jsonResponse(200, [firstTask, secondTask]));
    queue(jsonResponse(200, { ...firstTask, done: true }));
    render(<App />);
    await screen.findByText("buy milk");

    fireEvent.click(screen.getAllByRole("checkbox")[0]);

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(pathOf(1)).toBe("/api/tasks/1");
    expect(requestBody(1)).toEqual({ done: true });
    await waitFor(() => expect(screen.getAllByRole("checkbox")[0]).toBeChecked());
  });

  it("removes a task after a successful delete", async () => {
    queue(jsonResponse(200, [firstTask, secondTask]));
    queue(jsonResponse(200, { ok: true }));
    render(<App />);
    await screen.findByText("buy milk");

    fireEvent.click(screen.getByRole("button", { name: "Delete buy milk" }));

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(pathOf(1)).toBe("/api/tasks/1");
    await waitFor(() =>
      expect(screen.queryByText("buy milk")).not.toBeInTheDocument(),
    );
    expect(screen.getByText("write spec")).toBeInTheDocument();
  });

  it("shows the API error message when deletion fails", async () => {
    queue(jsonResponse(200, [firstTask, secondTask]));
    queue(jsonResponse(404, { error: "Task not found" }));
    render(<App />);
    await screen.findByText("buy milk");

    fireEvent.click(screen.getByRole("button", { name: "Delete buy milk" }));

    const banner = await screen.findByRole("alert");
    expect(banner).toHaveTextContent("Task not found");
    expect(screen.getByText("buy milk")).toBeInTheDocument();
  });
});
