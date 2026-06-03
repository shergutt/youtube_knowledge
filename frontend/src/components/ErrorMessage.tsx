import type { ApiErrorBody } from "../types/api";

interface ErrorMessageProps {
  error: { message: string; code?: string } | null;
}

export function ErrorMessage({ error }: ErrorMessageProps) {
  if (!error) return null;
  return (
    <div className="error-message" role="alert">
      <strong>Error</strong>
      <p>{error.message}</p>
      {error.code && <code>{error.code}</code>}
    </div>
  );
}

export function errorFromBody(body: ApiErrorBody | null | undefined): {
  message: string;
  code?: string;
} {
  if (!body) return { message: "Unknown error" };
  return { message: body.message, code: body.code };
}
