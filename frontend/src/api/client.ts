const BASE = import.meta.env.VITE_API_BASE ?? "";

export class ApiError extends Error {
  status: number;
  code: string;
  constructor(message: string, status: number, code: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(BASE + path, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
      ...(init?.headers ?? {}),
    },
  });
  if (!res.ok) {
    let code = "INTERNAL_ERROR";
    let message = `HTTP ${res.status}`;
    try {
      const body = await res.json();
      if (body?.error) {
        code = body.error.code ?? code;
        message = body.error.message ?? message;
      }
    } catch {
      // ignore
    }
    throw new ApiError(message, res.status, code);
  }
  if (res.status === 204) {
    return undefined as unknown as T;
  }
  const text = await res.text();
  if (!text) {
    return undefined as unknown as T;
  }
  return JSON.parse(text) as T;
}

export const client = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, body: unknown) =>
    request<T>(path, { method: "POST", body: JSON.stringify(body) }),
};
