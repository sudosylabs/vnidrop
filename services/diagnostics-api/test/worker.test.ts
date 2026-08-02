import { env, exports } from "cloudflare:workers";
import { createExecutionContext } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import worker from "../src/index";
import type { NormalizedBugPayload } from "../src/input";
import { type DiagnosticsEnv, runRetention, storeBug } from "../src/storage";

const INSTALL_ID = "10000000-0000-4000-8000-000000000000";

describe("diagnostics Worker", () => {
	it("keeps public routing narrow and fails closed", async () => {
		const live = await exports.default.fetch(new Request("https://diagnostics.test/live"));
		expect(live.status).toBe(200);
		expect(await live.json()).toMatchObject({ ok: true, schema: 1 });

		const health = await exports.default.fetch(healthRequest());
		expect(health.status).toBe(200);
		expect(await health.json()).toMatchObject({ ok: true });
		const unauthorizedHealth = await exports.default.fetch(healthRequest("wrong-key"));
		expect(unauthorizedHealth.status).toBe(401);

		const unknown = await exports.default.fetch(
			new Request("https://diagnostics.test/v1/not-real", { method: "POST" }),
		);
		expect(unknown.status).toBe(404);

		const unauthorized = await exports.default.fetch(
			jsonRequest("/v1/bugs", bugPayload(uuid(1), "logs"), "wrong-key"),
		);
		expect(unauthorized.status).toBe(401);
		expect(await unauthorized.json()).toEqual({ error: "unauthorized" });
		expect(unauthorized.headers.get("x-request-id")).toMatch(
			/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/,
		);

		const preflight = await exports.default.fetch(
			new Request("https://diagnostics.test/v1/bugs", { method: "OPTIONS" }),
		);
		expect(preflight.status).toBe(204);
		expect(preflight.headers.get("access-control-allow-origin")).toBeNull();
		expect(preflight.headers.get("allow")).toBe("POST, OPTIONS");
	});

	it("applies the source limit before shared-key verification", async () => {
		let installLimitCalls = 0;
		const limitedEnv: DiagnosticsEnv = {
			...env,
			SOURCE_RATE_LIMITER: {
				limit: async () => ({ success: false }),
			} as RateLimit,
			INSTALL_RATE_LIMITER: {
				limit: async () => {
					installLimitCalls += 1;
					return { success: true };
				},
			} as RateLimit,
		};
		const context = createExecutionContext();

		const response = await worker.fetch(
			jsonRequest("/v1/bugs", bugPayload(uuid(3), "logs"), "wrong-key", "198.51.100.3"),
			limitedEnv,
			context,
		);

		expect(response.status).toBe(429);
		expect(response.headers.get("retry-after")).toBe("60");
		expect(await response.json()).toEqual({ error: "rate_limited" });
		expect(installLimitCalls).toBe(0);
	});

	it("returns structured errors for invalid bodies and asynchronous storage failures", async () => {
		const invalid = await exports.default.fetch(jsonRequest("/v1/bugs", null));
		expect(invalid.status).toBe(400);
		expect(await invalid.json()).toEqual({ error: "invalid_body" });

		const rejection = new Error("simulated D1 rejection");
		const statement = {
			bind: () => statement,
			first: async () => Promise.reject(rejection),
			run: async () => Promise.reject(rejection),
		};
		const rejectingDatabase = {
			prepare: () => statement,
			batch: async () => Promise.reject(rejection),
			withSession: () => ({ prepare: () => statement }),
		} as unknown as D1Database;
		const rejectingEnv: DiagnosticsEnv = { ...env, DB: rejectingDatabase };

		const healthContext = createExecutionContext();
		const unhealthy = await worker.fetch(
			healthRequest(env.INGEST_KEY, "198.51.100.4"),
			rejectingEnv,
			healthContext,
		);
		expect(unhealthy.status).toBe(503);
		expect(await unhealthy.json()).toEqual({ ok: false, error: "dependency_unavailable" });

		const ingestContext = createExecutionContext();
		const failedIngest = await worker.fetch(
			jsonRequest("/v1/bugs", bugPayload(uuid(2), "logs"), env.INGEST_KEY, "198.51.100.2"),
			rejectingEnv,
			ingestContext,
		);
		expect(failedIngest.status).toBe(500);
		expect(await failedIngest.json()).toEqual({ error: "internal" });
	});

	it("stores bug metadata as JSON and cleans the duplicate upload attempt", async () => {
		const id = uuid(30);
		const payload = bugPayload(id, "first logs");
		const first = await exports.default.fetch(jsonRequest("/v1/bugs", payload));
		const second = await exports.default.fetch(
			jsonRequest("/v1/bugs", bugPayload(id, "second logs")),
		);

		expect(first.status).toBe(202);
		expect(await first.json()).toMatchObject({ ok: true, id, duplicate: false });
		expect(second.status).toBe(202);
		expect(await second.json()).toMatchObject({ ok: true, id, duplicate: true });

		const row = await env.DB.prepare(
			`SELECT occurred_at AS occurredAt, logs_r2_key AS logsKey,
			        device_json AS deviceJson
			 FROM bugs WHERE id = ?`,
		)
			.bind(id)
			.first<{
				occurredAt: number;
				logsKey: string;
				deviceJson: string;
			}>();
		expect(row?.occurredAt).toBe(3);
		expect(JSON.parse(row?.deviceJson ?? "null")).toEqual({
			deviceName: "Test device",
			deviceModel: "Model",
			operatingSystem: "Test OS",
			network: "offline",
			batteryLevel: "90%",
		});
		expect(await (await env.BLOBS.get(row?.logsKey ?? "missing"))?.text()).toBe("first logs");

		const objects = await env.BLOBS.list({ prefix: `bugs/${id}/` });
		expect(objects.objects.map((object) => object.key)).toEqual([row?.logsKey]);
	});

	it("acknowledges known report IDs without touching an unavailable blob store", async () => {
		const bug = normalizedBug(uuid(32), "accepted logs");
		await storeBug(bug, env);
		let blobWrites = 0;
		const unavailableBlobs = {
			put: async () => {
				blobWrites += 1;
				throw new Error("simulated R2 outage");
			},
		} as unknown as R2Bucket;
		const unavailableEnv: DiagnosticsEnv = { ...env, BLOBS: unavailableBlobs };

		await expect(
			storeBug({ ...bug, logs: "retry logs" }, unavailableEnv),
		).resolves.toEqual({ id: bug.id, duplicate: true, stored: 0 });
		expect(blobWrites).toBe(0);
	});

	it("removes uploaded report blobs when D1 rejects the metadata write", async () => {
		const rejection = new Error("simulated D1 write rejection");
		const rejectingDatabase = databaseWithSession(
			() => null,
			async () => Promise.reject(rejection),
		);
		const rejectingEnv: DiagnosticsEnv = { ...env, DB: rejectingDatabase };
		const bugId = uuid(34);

		const bugResponse = await worker.fetch(
			jsonRequest("/v1/bugs", bugPayload(bugId, "orphan candidate"), env.INGEST_KEY, "198.51.100.34"),
			rejectingEnv,
			createExecutionContext(),
		);

		expect(bugResponse.status).toBe(500);
		expect(await bugResponse.json()).toEqual({ error: "internal" });
		expect((await env.BLOBS.list({ prefix: `bugs/${bugId}/` })).objects).toEqual([]);
	});

	it("removes expired rows and their exact R2 objects while preserving current data", async () => {
		const oldBugId = uuid(42);
		const currentBugId = uuid(43);
		const oldBugKey = `bugs/${oldBugId}/retention/logs.txt`;
		const oldReceivedAt = Date.now() - 100 * 86_400_000;

		await env.BLOBS.put(oldBugKey, "expired logs");
		await env.DB.batch([
			env.DB.prepare(
				`INSERT INTO bugs
				 (id, received_at, occurred_at, install_id, app_version, platform,
				  what_happened, expected, steps, contact, logs_r2_key,
				  device_json, status, schema_version)
				 VALUES (?, ?, ?, ?, '', '', 'failed', 'worked', '', '', ?, '{}', 'open', 1)`,
			).bind(oldBugId, oldReceivedAt, oldReceivedAt, INSTALL_ID, oldBugKey),
			env.DB.prepare(
				`INSERT INTO bugs
				 (id, received_at, occurred_at, install_id, app_version, platform,
				  what_happened, expected, steps, contact, logs_r2_key,
				  device_json, status, schema_version)
				 VALUES (?, ?, ?, ?, '', '', 'failed', 'worked', '', '', NULL, '{}', 'open', 1)`,
			).bind(currentBugId, Date.now(), Date.now(), INSTALL_ID),
		]);

		await runRetention(env);

		expect(
			await env.DB.prepare("SELECT id FROM bugs WHERE id = ?").bind(oldBugId).first(),
		).toBeNull();
		expect(await env.BLOBS.head(oldBugKey)).toBeNull();
		expect(
			await env.DB.prepare("SELECT id FROM bugs WHERE id = ?").bind(currentBugId).first(),
		).not.toBeNull();
	});

	it("bounds a full retention run below the D1 per-invocation query limit", async () => {
		let queryCount = 0;
		const blobDeleteBatchSizes: number[] = [];
		const rows = Array.from({ length: 900 }, (_, index) => ({
			id: `expired-${index}`,
			blobKey: `expired/${index}`,
		}));
		const database = {
			prepare: () => {
				const statement = {
					bind: () => statement,
					all: async () => {
						queryCount += 1;
						return d1Result(rows, 0);
					},
					run: async () => {
						queryCount += 1;
						return d1Result([], rows.length);
					},
					first: async () => {
						queryCount += 1;
						return { count: rows.length };
					},
				};
				return statement;
			},
		} as unknown as D1Database;
		const warning = vi.spyOn(console, "warn").mockImplementation(() => undefined);
		const blobs = {
			delete: async (keys: string | string[]) => {
				blobDeleteBatchSizes.push(typeof keys === "string" ? 1 : keys.length);
			},
		} as unknown as R2Bucket;

		try {
			await runRetention({ ...env, DB: database, BLOBS: blobs });
		} finally {
			warning.mockRestore();
		}

		// Eight passes (one SELECT + one DELETE each) plus the final backlog SELECT.
		expect(queryCount).toBe(17);
		expect(blobDeleteBatchSizes).toHaveLength(8);
		expect(Math.max(...blobDeleteBatchSizes)).toBe(900);
	});

	it("converges an expired report backlog across bounded retention runs", async () => {
		const oldReceivedAt = Date.now() - 100 * 86_400_000;
		await env.DB.prepare(
			`WITH digits(value) AS (
			   VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)
			 ), sequence(value) AS (
			   SELECT thousands.value * 1000 + hundreds.value * 100 + tens.value * 10 + ones.value + 1
			   FROM digits AS thousands
			   CROSS JOIN digits AS hundreds
			   CROSS JOIN digits AS tens
			   CROSS JOIN digits AS ones
			   WHERE thousands.value * 1000 + hundreds.value * 100 + tens.value * 10 + ones.value < 7201
			 )
			 INSERT INTO bugs (
			   id, received_at, occurred_at, install_id, app_version, platform,
			   what_happened, expected, steps, contact, logs_r2_key,
			   device_json, status, schema_version
			 )
			 SELECT 'retention-backlog-' || printf('%04d', value), ?, ?, ?, '', '',
			        'failed', 'worked', '', '', NULL, '{}', 'open', 1
			 FROM sequence`,
		)
			.bind(oldReceivedAt, oldReceivedAt, INSTALL_ID)
			.run();
		const warning = vi.spyOn(console, "warn").mockImplementation(() => undefined);

		try {
			await runRetention(env);
			const afterFirstRun = await env.DB.prepare(
				"SELECT COUNT(*) AS count FROM bugs WHERE id LIKE 'retention-backlog-%'",
			).first<{ count: number }>();
			expect(afterFirstRun?.count).toBe(1);

			await runRetention(env);
			const afterSecondRun = await env.DB.prepare(
				"SELECT COUNT(*) AS count FROM bugs WHERE id LIKE 'retention-backlog-%'",
			).first<{ count: number }>();
			expect(afterSecondRun?.count).toBe(0);
		} finally {
			warning.mockRestore();
		}
	});
});

function jsonRequest(
	path: string,
	body: unknown,
	key = env.INGEST_KEY,
	source = "198.51.100.1",
): Request {
	return new Request(`https://diagnostics.test${path}`, {
		method: "POST",
		headers: {
			"content-type": "application/json; charset=utf-8",
			"cf-connecting-ip": source,
			"x-vnidrop-install-id": INSTALL_ID,
			"x-vnidrop-key": key,
		},
		body: JSON.stringify(body),
	});
}

function healthRequest(key = env.INGEST_KEY, source = "198.51.100.1"): Request {
	return new Request("https://diagnostics.test/health", {
		headers: {
			"cf-connecting-ip": source,
			"x-vnidrop-key": key,
		},
	});
}

function bugPayload(id: string, logs: string): Record<string, unknown> {
	return {
		id,
		installId: INSTALL_ID,
		appVersion: "1.0",
		platform: "test",
		timestampMillis: 3,
		whatHappened: "It failed",
		expected: "It should work",
		steps: "Open the app",
		contact: "",
		includeLogs: true,
		logs,
		device: {
			deviceName: "Test device",
			deviceModel: "Model",
			operatingSystem: "Test OS",
			network: "offline",
			batteryLevel: "90%",
		},
		schemaVersion: 1,
	};
}

function normalizedBug(id: string, logs: string): NormalizedBugPayload {
	return {
		id,
		installId: INSTALL_ID,
		appVersion: "1.0",
		platform: "test",
		occurredAt: 3,
		whatHappened: "It failed",
		expected: "It should work",
		steps: "Open the app",
		contact: "",
		logs,
		device: {
			deviceName: "Test device",
			deviceModel: "Model",
			operatingSystem: "Test OS",
			network: "offline",
			batteryLevel: "90%",
		},
		schemaVersion: 1,
	};
}

function databaseWithSession(
	first: () => unknown,
	run: () => Promise<D1Result>,
): D1Database {
	const statement = {
		bind: () => statement,
		first: async () => first(),
		run,
	} as unknown as D1PreparedStatement;
	const session = {
		prepare: () => statement,
	} as unknown as D1DatabaseSession;
	return {
		withSession: () => session,
	} as unknown as D1Database;
}

function d1Result<T>(results: T[], changes: number): D1Result<T> {
	return { success: true, results, meta: { changes } } as D1Result<T>;
}

function uuid(suffix: number): string {
	return `00000000-0000-4000-8000-${suffix.toString().padStart(12, "0")}`;
}
