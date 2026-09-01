import { Platform } from 'react-native';
import type { ZodType } from 'zod';

import { Problem as ProblemSchema } from '@/generated/public-api/model/problem.zod';

const DEFAULT_TIMEOUT_MS = 10_000;

type FrameworkRequestInit = RequestInit & {
  schema?: ZodType;
  timeoutMs?: number;
};

export class ContractViolationError extends Error {
  readonly kind = 'contractViolation' as const;

  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'ContractViolationError';
  }
}

export class TransportError extends Error {
  readonly kind = 'transport' as const;

  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'TransportError';
  }
}

export class CancelledError extends Error {
  readonly kind = 'cancelled' as const;

  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'CancelledError';
  }
}

export class TimedOutError extends Error {
  readonly kind = 'timedOut' as const;

  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'TimedOutError';
  }
}

/**
 * Framework-owned transport boundary used by generated public-client code.
 *
 * Orval supplies the operation's generated success schema. Error payloads use
 * the single RFC 9457 Problem schema. The operation facade remains responsible
 * for accepting only the status codes declared by that operation.
 */
export async function frameworkFetch<T>(
  relativeUrl: string,
  { schema, timeoutMs = DEFAULT_TIMEOUT_MS, signal, ...request }: FrameworkRequestInit,
): Promise<T> {
  const timeoutController = new AbortController();
  const timeout = setTimeout(() => timeoutController.abort(), timeoutMs);
  const combinedSignal = combineSignals(signal, timeoutController.signal);

  try {
    const response = await fetch(resolveApiUrl(relativeUrl), {
      ...request,
      signal: combinedSignal,
    });
    const data = await parseResponse(response, schema);

    return {
      data,
      status: response.status,
      headers: response.headers,
    } as T;
  } catch (error) {
    if (error instanceof ContractViolationError) {
      throw error;
    }
    if (timeoutController.signal.aborted) {
      throw new TimedOutError(`Request timed out after ${timeoutMs} ms`, { cause: error });
    }
    if (signal?.aborted || isAbortError(error)) {
      throw new CancelledError('Request was cancelled', { cause: error });
    }
    throw new TransportError('The API could not be reached', { cause: error });
  } finally {
    clearTimeout(timeout);
  }
}

async function parseResponse(response: Response, schema?: ZodType): Promise<unknown> {
  const contentType = response.headers.get('content-type')?.toLowerCase() ?? '';

  if (response.ok) {
    if (!contentType.includes('application/json')) {
      throw new ContractViolationError(
        `Expected application/json for HTTP ${response.status}, received ${contentType || 'no content type'}`,
      );
    }
    if (!schema) {
      throw new ContractViolationError('Generated client did not provide a success schema');
    }

    const payload = await parseJson(response);
    const parsed = schema.safeParse(payload);
    if (!parsed.success) {
      throw new ContractViolationError('Success response did not match its generated schema', {
        cause: parsed.error,
      });
    }
    return parsed.data;
  }

  if (!contentType.includes('application/problem+json')) {
    throw new ContractViolationError(
      `Expected application/problem+json for HTTP ${response.status}, received ${contentType || 'no content type'}`,
    );
  }

  const parsed = ProblemSchema.safeParse(await parseJson(response));
  if (!parsed.success) {
    throw new ContractViolationError('Error response did not match the generated Problem schema', {
      cause: parsed.error,
    });
  }
  if (parsed.data.status !== response.status) {
    throw new ContractViolationError(
      `Problem status ${parsed.data.status} disagrees with HTTP status ${response.status}`,
    );
  }
  return parsed.data;
}

async function parseJson(response: Response): Promise<unknown> {
  const body = await response.text();
  try {
    return JSON.parse(body);
  } catch (error) {
    throw new ContractViolationError(`HTTP ${response.status} body was not valid JSON`, {
      cause: error,
    });
  }
}

function resolveApiUrl(relativeUrl: string): string {
  const configuredBaseUrl = process.env.EXPO_PUBLIC_API_URL?.trim();
  if (configuredBaseUrl) {
    return new URL(relativeUrl, ensureTrailingSlash(configuredBaseUrl)).toString();
  }

  if (Platform.OS === 'web') {
    return relativeUrl;
  }

  const host = Platform.OS === 'android' ? '10.0.2.2' : '127.0.0.1';
  return new URL(relativeUrl, `http://${host}:4000`).toString();
}

function ensureTrailingSlash(value: string): string {
  return value.endsWith('/') ? value : `${value}/`;
}

function combineSignals(
  externalSignal: AbortSignal | null | undefined,
  timeoutSignal: AbortSignal,
): AbortSignal {
  if (!externalSignal) {
    return timeoutSignal;
  }
  return AbortSignal.any([externalSignal, timeoutSignal]);
}

function isAbortError(error: unknown): boolean {
  return error instanceof Error && error.name === 'AbortError';
}
