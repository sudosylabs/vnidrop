import {
	normalizeBug,
	readJsonObject,
} from "./input";
import {
	type DiagnosticsEnv,
	runRetention,
	storeBug,
} from "./storage";

const DEFAULT_MAX_BODY_BYTES = 262_144;
const HARD_MAX_BODY_BYTES = 1_048_576;

export default {
	async fetch(request: Request, env: DiagnosticsEnv, _ctx: ExecutionContext): Promise<Response> {
		const requestId = crypto.randomUUID();
		const url = new URL(request.url);
		try {
			if (request.method === "GET" && url.pathname === "/live") {
				return json({ ok: true, service: "vnidrop-diagnostics", schema: 1 }, 200, requestId);
			}
			if (request.method === "GET" && url.pathname === "/health") {
				if (await sourceRateLimited(request, url.pathname, env)) {
					return json({ error: "rate_limited" }, 429, requestId, { "retry-after": "60" });
				}
				const authError = await authorize(request, env);
				if (authError) return json({ error: authError.error }, authError.status, requestId);
				return await readiness(env, requestId);
			}

			if (!isIngestPath(url.pathname)) {
				return json({ error: "not_found" }, 404, requestId);
			}
			if (request.method === "OPTIONS") {
				return new Response(null, {
					status: 204,
					headers: responseHeaders(requestId, { allow: "POST, OPTIONS" }),
				});
			}
			if (request.method !== "POST") {
				return json(
					{ error: "method_not_allowed" },
					405,
					requestId,
					{ allow: "POST, OPTIONS" },
				);
			}

			if (await sourceRateLimited(request, url.pathname, env)) {
				return json({ error: "rate_limited" }, 429, requestId, { "retry-after": "60" });
			}

			const authError = await authorize(request, env);
			if (authError) return json({ error: authError.error }, authError.status, requestId);

			if (await installRateLimited(request, url.pathname, env)) {
				return json({ error: "rate_limited" }, 429, requestId, { "retry-after": "60" });
			}

			const maxBodyBytes = boundedPositiveInt(
				env.MAX_BODY_BYTES,
				DEFAULT_MAX_BODY_BYTES,
				1,
				HARD_MAX_BODY_BYTES,
			);
			const parsed = await readJsonObject(request, maxBodyBytes);
			if (!parsed.ok) return json({ error: parsed.error }, parsed.status, requestId);

			switch (url.pathname) {
				case "/v1/bugs": {
					const normalized = normalizeBug(parsed.value);
					if (!normalized.ok) {
						return json({ error: normalized.error }, normalized.status, requestId);
					}
					const result = await storeBug(normalized.value, env);
					return json(
						{ ok: true, id: result.id, duplicate: result.duplicate },
						202,
						requestId,
					);
				}
			}
		} catch (error) {
			console.error(
				JSON.stringify({
					message: "diagnostics request failed",
					requestId,
					method: request.method,
					path: url.pathname,
					error: error instanceof Error ? error.message : String(error),
				}),
			);
			return json({ error: "internal" }, 500, requestId);
		}
	},

	scheduled(
		controller: ScheduledController,
		env: DiagnosticsEnv,
		ctx: ExecutionContext,
	): void {
		ctx.waitUntil(
			runRetention(env).catch((error) => {
				console.error(
					JSON.stringify({
						message: "diagnostics retention failed",
						scheduledTime: controller.scheduledTime,
						error: error instanceof Error ? error.message : String(error),
					}),
				);
			}),
		);
	},
} satisfies ExportedHandler<DiagnosticsEnv>;

async function readiness(env: DiagnosticsEnv, requestId: string): Promise<Response> {
	if (!env.INGEST_KEY) {
		return json({ ok: false, error: "server_misconfigured" }, 503, requestId);
	}
	try {
		await env.DB.batch([
			env.DB.prepare(
				"SELECT id, occurred_at, logs_r2_key, device_json FROM bugs LIMIT 1",
			),
		]);
		return json({ ok: true, service: "vnidrop-diagnostics", schema: 1 }, 200, requestId);
	} catch (error) {
		console.error(
			JSON.stringify({
				message: "diagnostics readiness check failed",
				requestId,
				error: error instanceof Error ? error.message : String(error),
			}),
		);
		return json({ ok: false, error: "dependency_unavailable" }, 503, requestId);
	}
}

async function authorize(
	request: Request,
	env: DiagnosticsEnv,
): Promise<{ status: 401 | 503; error: "unauthorized" | "server_misconfigured" } | null> {
	const expected = env.INGEST_KEY;
	if (!expected) return { status: 503, error: "server_misconfigured" };
	const provided = request.headers.get("x-vnidrop-key") ?? "";
	if (!(await timingSafeEqual(provided, expected))) {
		return { status: 401, error: "unauthorized" };
	}
	return null;
}

async function sourceRateLimited(
	request: Request,
	path: string,
	env: DiagnosticsEnv,
): Promise<boolean> {
	const source = request.headers.get("cf-connecting-ip")?.slice(0, 64) || "unknown";
	const result = await env.SOURCE_RATE_LIMITER.limit({ key: `${source}:${path}` });
	return !result.success;
}

async function installRateLimited(
	request: Request,
	path: string,
	env: DiagnosticsEnv,
): Promise<boolean> {
	const source = request.headers.get("cf-connecting-ip")?.slice(0, 64) || "unknown";
	const installId = request.headers.get("x-vnidrop-install-id")?.trim().slice(0, 80) || source;
	const result = await env.INSTALL_RATE_LIMITER.limit({ key: `${installId}:${path}` });
	return !result.success;
}

function isIngestPath(path: string): path is "/v1/bugs" {
	return path === "/v1/bugs";
}

async function timingSafeEqual(provided: string, expected: string): Promise<boolean> {
	const encoder = new TextEncoder();
	const [providedHash, expectedHash] = await Promise.all([
		crypto.subtle.digest("SHA-256", encoder.encode(provided)),
		crypto.subtle.digest("SHA-256", encoder.encode(expected)),
	]);
	return crypto.subtle.timingSafeEqual(providedHash, expectedHash);
}

function json(
	body: unknown,
	status: number,
	requestId: string,
	extraHeaders?: Readonly<Record<string, string>>,
): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: responseHeaders(requestId, {
			"content-type": "application/json; charset=utf-8",
			...extraHeaders,
		}),
	});
}

function responseHeaders(
	requestId: string,
	extraHeaders?: Readonly<Record<string, string>>,
): Headers {
	const headers = new Headers(extraHeaders);
	headers.set("cache-control", "no-store");
	headers.set("x-content-type-options", "nosniff");
	headers.set("x-request-id", requestId);
	return headers;
}

function boundedPositiveInt(
	raw: string | undefined,
	fallback: number,
	minimum: number,
	maximum: number,
): number {
	const parsed = Number(raw);
	if (!Number.isSafeInteger(parsed) || parsed < minimum || parsed > maximum) return fallback;
	return parsed;
}
