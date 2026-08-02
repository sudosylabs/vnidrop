import type { NormalizedBugPayload } from "./input";

export type DiagnosticsEnv = Cloudflare.Env & {
	INGEST_KEY?: string;
};

export interface StoreResult {
	id: string;
	duplicate: boolean;
	stored: number;
}

export async function storeBug(
	payload: NormalizedBugPayload,
	env: DiagnosticsEnv,
): Promise<StoreResult> {
	const database = env.DB.withSession("first-primary");
	const existing = await database
		.prepare("SELECT id FROM bugs WHERE id = ?")
		.bind(payload.id)
		.first<{ id: string }>();
	if (existing) return { id: payload.id, duplicate: true, stored: 0 };

	const logsKey = payload.logs ? `bugs/${payload.id}/${crypto.randomUUID()}/logs.txt` : null;
	if (logsKey) {
		await env.BLOBS.put(logsKey, payload.logs, {
			httpMetadata: { contentType: "text/plain; charset=utf-8" },
			customMetadata: { installId: payload.installId },
		});
	}

	try {
		const result = await database
			.prepare(
			`INSERT INTO bugs (
			  id, received_at, occurred_at, install_id, app_version, platform,
			  what_happened, expected, steps, contact, logs_r2_key,
			  device_json, breadcrumbs_json, status, schema_version
			) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', ?)
			ON CONFLICT(id) DO NOTHING`,
			)
			.bind(
				payload.id,
				Date.now(),
				payload.occurredAt,
				payload.installId,
				payload.appVersion,
				payload.platform,
				payload.whatHappened,
				payload.expected,
				payload.steps,
				payload.contact,
				logsKey,
				JSON.stringify(payload.device),
				JSON.stringify(payload.breadcrumbs),
				payload.schemaVersion,
			)
			.run();
		const duplicate = result.meta.changes === 0;
		if (duplicate && logsKey) {
			await deleteAttemptBlob(env, logsKey);
		}
		return { id: payload.id, duplicate, stored: duplicate ? 0 : 1 };
	} catch (error) {
		if (logsKey) {
			await deleteAttemptBlob(env, logsKey);
		}
		throw error;
	}
}

export async function runRetention(env: DiagnosticsEnv): Promise<void> {
	const retentionDays = boundedPositiveInt(env.RETENTION_DAYS, 90, 1, 3_650);
	const cutoff = Date.now() - retentionDays * 86_400_000;
	// Eight passes plus the backlog check stay well within D1's 50 queries per invocation.
	for (let pass = 0; pass < 8; pass += 1) {
		const hasFullBatch = await runRetentionPass(env, cutoff);
		if (!hasFullBatch) return;
	}
	const bugs = await env.DB.prepare(
		"SELECT COUNT(*) AS count FROM bugs WHERE received_at < ?",
	)
		.bind(cutoff)
		.first<{ count: number }>();
	console.warn(
		JSON.stringify({
			message: "diagnostics retention reached its per-run pass limit",
			cutoff,
			backlog: {
				bugs: bugs?.count ?? 0,
			},
		}),
	);
}

async function runRetentionPass(env: DiagnosticsEnv, cutoff: number): Promise<boolean> {
	const reportBatchSize = 900;
	const bugs = await expiredBlobRows(env.DB, "bugs", "logs_r2_key", cutoff, reportBatchSize);

	const blobKeys = bugs
		.map((row) => row.blobKey)
		.filter((key): key is string => key !== null);
	for (let offset = 0; offset < blobKeys.length; offset += 1_000) {
		await env.BLOBS.delete(blobKeys.slice(offset, offset + 1_000));
	}

	if (bugs.length > 0) await deleteRowsById(env.DB, "bugs", bugs).run();
	return bugs.length === reportBatchSize;
}

interface ExpiredBlobRow {
	id: string;
	blobKey: string | null;
}

async function expiredBlobRows(
	database: D1Database,
	table: "bugs",
	column: "logs_r2_key",
	cutoff: number,
	batchSize: number,
): Promise<ExpiredBlobRow[]> {
	const result = await database
		.prepare(
			`SELECT id, ${column} AS blobKey
			 FROM ${table}
			 WHERE received_at < ?
			 ORDER BY received_at
			 LIMIT ?`,
		)
		.bind(cutoff, batchSize)
		.all<ExpiredBlobRow>();
	return result.results;
}

function deleteRowsById(
	database: D1Database,
	table: "bugs",
	rows: ExpiredBlobRow[],
): D1PreparedStatement {
	return database
		.prepare(`DELETE FROM ${table} WHERE id IN (SELECT value FROM json_each(?))`)
		.bind(JSON.stringify(rows.map((row) => row.id)));
}

async function deleteAttemptBlob(env: DiagnosticsEnv, key: string): Promise<void> {
	try {
		await env.BLOBS.delete(key);
	} catch (error) {
		console.error(
			JSON.stringify({
				message: "failed to remove uncommitted diagnostics blob",
				error: error instanceof Error ? error.message : String(error),
			}),
		);
	}
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
