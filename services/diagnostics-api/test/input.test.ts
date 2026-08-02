import { describe, expect, it } from "vitest";
import {
	MAX_DEVICE_JSON_BYTES,
	MAX_LOG_BYTES,
	normalizeBug,
	readJsonObject,
} from "../src/input";

const ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const INSTALL_ID = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const ENCODER = new TextEncoder();

describe("readJsonObject", () => {
	it("enforces the byte limit on a chunked body without Content-Length", async () => {
		const bytes = ENCODER.encode(JSON.stringify({ value: "😀".repeat(40) }));
		const request = chunkedJsonRequest([
			bytes.subarray(0, 7),
			bytes.subarray(7, 31),
			bytes.subarray(31),
		]);

		expect(request.headers.has("content-length")).toBe(false);
		await expect(readJsonObject(request, 64)).resolves.toEqual({
			ok: false,
			status: 413,
			error: "payload_too_large",
		});
	});

	it("joins chunks before fatally decoding UTF-8", async () => {
		const bytes = ENCODER.encode(JSON.stringify({ value: "😀" }));
		const emojiStart = bytes.indexOf(0xf0);
		const request = chunkedJsonRequest([
			bytes.subarray(0, emojiStart + 2),
			bytes.subarray(emojiStart + 2),
		]);

		await expect(readJsonObject(request, bytes.byteLength)).resolves.toEqual({
			ok: true,
			value: { value: "😀" },
		});
	});

	it("rejects malformed UTF-8", async () => {
		const request = chunkedJsonRequest([
			new Uint8Array([0x7b, 0x22, 0x78, 0x22, 0x3a, 0x22, 0xc3, 0x28, 0x22, 0x7d]),
		]);

		await expect(readJsonObject(request, 100)).resolves.toEqual({
			ok: false,
			status: 400,
			error: "invalid_utf8",
		});
	});

	it("requires application/json with a UTF-8 charset", async () => {
		const missing = new Request("https://example.test/v1/bugs", {
			method: "POST",
			body: "{}",
		});
		const wrongCharset = chunkedJsonRequest([ENCODER.encode("{}")], "application/json; charset=utf-16");

		await expect(readJsonObject(missing, 100)).resolves.toEqual({
			ok: false,
			status: 415,
			error: "unsupported_media_type",
		});
		await expect(readJsonObject(wrongCharset, 100)).resolves.toEqual({
			ok: false,
			status: 415,
			error: "unsupported_media_type",
		});
	});

	it("validates Content-Length before reading the body", async () => {
		const invalid = chunkedJsonRequest([ENCODER.encode("{}")], "application/json", "invalid");
		const oversized = chunkedJsonRequest(
			[ENCODER.encode("{}")],
			"application/json",
			"999999999999999999999999999999999999",
		);

		await expect(readJsonObject(invalid, 100)).resolves.toEqual({
			ok: false,
			status: 400,
			error: "invalid_content_length",
		});
		await expect(readJsonObject(oversized, 100)).resolves.toEqual({
			ok: false,
			status: 413,
			error: "payload_too_large",
		});
	});

	it("rejects a non-object JSON root", async () => {
		const request = chunkedJsonRequest([ENCODER.encode("null")]);

		await expect(readJsonObject(request, 100)).resolves.toEqual({
			ok: false,
			status: 400,
			error: "invalid_body",
		});
	});
});

describe("normalizers", () => {
	it("preserves false booleans and rejects their string representation", () => {
		const bug = bugPayload({ include_logs: false, logs: "discard me" });
		const normalizedBug = normalizeBug(bug);
		expect(normalizedBug.ok).toBe(true);
		if (normalizedBug.ok) expect(normalizedBug.value.logs).toBe("");
		const missingConsent = normalizeBug(bugPayload({ logs: "discard me" }));
		expect(missingConsent.ok && missingConsent.value.logs).toBe("");
		expect(normalizeBug(bugPayload({ include_logs: "false" }))).toEqual({
			ok: false,
			status: 400,
			error: "invalid_include_logs",
		});
	});

	it("keeps logs and device JSON within valid byte budgets", () => {
		const result = normalizeBug(
			bugPayload({
				include_logs: true,
				logs: "😀".repeat(60_000),
				device: {
					device_name: "\u0000".repeat(200),
					device_model: "\u0000".repeat(200),
					operating_system: "\u0000".repeat(300),
					network: "\u0000".repeat(150),
					battery_level: "\u0000".repeat(100),
				},
			}),
		);

		expect(result.ok).toBe(true);
		if (!result.ok) return;
		const deviceJson = JSON.stringify(result.value.device);
		expect(ENCODER.encode(result.value.logs).byteLength).toBe(MAX_LOG_BYTES);
		expect(ENCODER.encode(deviceJson).byteLength).toBeLessThanOrEqual(MAX_DEVICE_JSON_BYTES);
		expect(JSON.parse(deviceJson)).toEqual(result.value.device);
	});

	it("requires stable report IDs and validates supplied IDs and schema versions", () => {
		const missingId = normalizeBug(bugPayload({ id: undefined }));
		expect(missingId).toEqual({ ok: false, status: 400, error: "invalid_id" });

		const legacyInstall = normalizeBug(bugPayload({ install_id: "legacy-test-install" }));
		expect(legacyInstall.ok && legacyInstall.value.installId).toBe("legacy-test-install");

		const missingInstall = normalizeBug(bugPayload({ install_id: undefined }));
		expect(missingInstall.ok && missingInstall.value.installId).toBe("unknown");

		expect(normalizeBug(bugPayload({ install_id: "bad\u0000install" }))).toEqual({
			ok: false,
			status: 400,
			error: "invalid_install_id",
		});

		expect(normalizeBug(bugPayload({ id: "not-a-uuid" }))).toEqual({
			ok: false,
			status: 400,
			error: "invalid_id",
		});
		expect(normalizeBug(bugPayload({ schema_version: 2 }))).toEqual({
			ok: false,
			status: 400,
			error: "unsupported_schema_version",
		});
	});
});

function chunkedJsonRequest(
	chunks: readonly Uint8Array[],
	contentType = "application/json; charset=utf-8",
	contentLength?: string,
): Request {
	return new Request("https://example.test/v1/bugs", {
		method: "POST",
		headers: {
			"content-type": contentType,
			...(contentLength == null ? {} : { "content-length": contentLength }),
		},
		body: new ReadableStream<Uint8Array>({
			start(controller) {
				for (const chunk of chunks) controller.enqueue(chunk);
				controller.close();
			},
		}),
	});
}

function bugPayload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
	const payload: Record<string, unknown> = {
		id: ID,
		install_id: INSTALL_ID,
		app_version: "1.0",
		platform: "test",
		occurred_at: 1,
		what_happened: "It failed",
		expected: "It worked",
		steps: "Open the app",
		contact: "",
		logs: "",
		device: {},
		schema_version: 1,
		...overrides,
	};
	// An explicit `undefined` override omits the key entirely (simulating a missing field).
	for (const key of Object.keys(overrides)) {
		if (overrides[key] === undefined) delete payload[key];
	}
	return payload;
}
