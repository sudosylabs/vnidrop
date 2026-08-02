export type JsonObject = Record<string, unknown>;

export type InputFailure = {
	ok: false;
	status: 400 | 413 | 415;
	error: string;
};

export type InputResult<T> = { ok: true; value: T } | InputFailure;

export interface NormalizedDevice {
	deviceName: string;
	deviceModel: string;
	operatingSystem: string;
	network: string;
	batteryLevel: string;
}

export interface NormalizedBugPayload {
	id: string;
	installId: string;
	appVersion: string;
	platform: string;
	occurredAt: number;
	whatHappened: string;
	expected: string;
	steps: string;
	contact: string;
	logs: string;
	device: NormalizedDevice;
	schemaVersion: 1;
}

export const MAX_LOG_BYTES = 192 * 1024;
export const MAX_DEVICE_JSON_BYTES = 4_000;

const MISSING = Symbol("missing");
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const UTF8_ENCODER = new TextEncoder();

export async function readJsonObject(
	request: Request,
	maxBytes: number,
): Promise<InputResult<JsonObject>> {
	if (!Number.isSafeInteger(maxBytes) || maxBytes <= 0) {
		throw new RangeError("maxBytes must be a positive safe integer");
	}

	if (!isApplicationJson(request.headers.get("content-type"))) {
		return failure(415, "unsupported_media_type");
	}

	const contentLength = request.headers.get("content-length");
	if (contentLength != null) {
		const trimmed = contentLength.trim();
		if (!/^\d+$/.test(trimmed)) {
			return failure(400, "invalid_content_length");
		}
		const normalizedLength = trimmed.replace(/^0+(?=\d)/, "");
		const maxLength = String(maxBytes);
		if (
			normalizedLength.length > maxLength.length ||
			(normalizedLength.length === maxLength.length && normalizedLength > maxLength)
		) {
			return failure(413, "payload_too_large");
		}
		const declaredBytes = Number(trimmed);
		if (!Number.isSafeInteger(declaredBytes)) {
			return failure(400, "invalid_content_length");
		}
	}

	if (request.body == null) {
		return failure(400, "invalid_json");
	}

	let reader: ReadableStreamDefaultReader<Uint8Array>;
	try {
		reader = request.body.getReader();
	} catch {
		return failure(400, "invalid_body");
	}
	const chunks: Uint8Array[] = [];
	let totalBytes = 0;
	try {
		while (true) {
			const chunk = await reader.read();
			if (chunk.done) break;
			if (chunk.value.byteLength > maxBytes - totalBytes) {
				try {
					await reader.cancel("payload_too_large");
				} catch {
					// The size failure remains authoritative if cancellation also fails.
				}
				return failure(413, "payload_too_large");
			}
			totalBytes += chunk.value.byteLength;
			chunks.push(chunk.value);
		}
	} catch {
		return failure(400, "invalid_body");
	} finally {
		reader.releaseLock();
	}

	const bytes = joinChunks(chunks, totalBytes);
	let raw: string;
	try {
		raw = new TextDecoder("utf-8", { fatal: true, ignoreBOM: false }).decode(bytes);
	} catch {
		return failure(400, "invalid_utf8");
	}

	let parsed: unknown;
	try {
		parsed = JSON.parse(raw);
	} catch {
		return failure(400, "invalid_json");
	}
	if (!isPlainObject(parsed)) {
		return failure(400, "invalid_body");
	}
	return success(parsed);
}

export function normalizeBug(body: JsonObject): InputResult<NormalizedBugPayload> {
	if (!isPlainObject(body)) return failure(400, "invalid_body");

	const id = idField(body, ["id"], "invalid_id");
	if (!id.ok) return id;
	const installId = installIdField(body);
	if (!installId.ok) return installId;
	const appVersion = stringField(body, ["appVersion", "app_version"], 40, "invalid_app_version");
	if (!appVersion.ok) return appVersion;
	const platform = stringField(body, ["platform"], 40, "invalid_platform");
	if (!platform.ok) return platform;
	const occurredAt = timestampField(
		body,
		["timestampMillis", "timestamp_millis", "occurredAt", "occurred_at"],
	);
	if (!occurredAt.ok) return occurredAt;
	const whatHappened = stringField(
		body,
		["whatHappened", "what_happened"],
		4_000,
		"missing_fields",
		true,
		true,
	);
	if (!whatHappened.ok) return whatHappened;
	const expected = stringField(body, ["expected"], 4_000, "missing_fields", true, true);
	if (!expected.ok) return expected;
	const steps = stringField(body, ["steps"], 4_000, "invalid_steps");
	if (!steps.ok) return steps;
	const contact = stringField(body, ["contact"], 320, "invalid_contact");
	if (!contact.ok) return contact;
	const includeLogs = booleanField(
		body,
		["includeLogs", "include_logs"],
		"invalid_include_logs",
		false,
	);
	if (!includeLogs.ok) return includeLogs;
	const logs = stringField(body, ["logs"], MAX_LOG_BYTES, "invalid_logs");
	if (!logs.ok) return logs;
	const device = normalizeDevice(pick(body, ["device"]));
	if (!device.ok) return device;
	const version = schemaVersion(body);
	if (!version.ok) return version;

	return success({
		id: id.value,
		installId: installId.value,
		appVersion: appVersion.value,
		platform: platform.value,
		occurredAt: occurredAt.value,
		whatHappened: whatHappened.value,
		expected: expected.value,
		steps: steps.value,
		contact: contact.value,
		logs: includeLogs.value === true ? logs.value : "",
		device: device.value,
		schemaVersion: version.value,
	});
}

function normalizeDevice(raw: unknown | typeof MISSING): InputResult<NormalizedDevice> {
	if (raw === MISSING) raw = {};
	if (!isPlainObject(raw)) return failure(400, "invalid_device");

	const deviceName = stringField(raw, ["deviceName", "device_name"], 128, "invalid_device");
	if (!deviceName.ok) return deviceName;
	const deviceModel = stringField(raw, ["deviceModel", "device_model"], 128, "invalid_device");
	if (!deviceModel.ok) return deviceModel;
	const operatingSystem = stringField(
		raw,
		["operatingSystem", "operating_system"],
		192,
		"invalid_device",
	);
	if (!operatingSystem.ok) return operatingSystem;
	const network = stringField(raw, ["network"], 96, "invalid_device");
	if (!network.ok) return network;
	const batteryLevel = stringField(raw, ["batteryLevel", "battery_level"], 64, "invalid_device");
	if (!batteryLevel.ok) return batteryLevel;

	const device: NormalizedDevice = {
		deviceName: deviceName.value,
		deviceModel: deviceModel.value,
		operatingSystem: operatingSystem.value,
		network: network.value,
		batteryLevel: batteryLevel.value,
	};
	if (jsonBytes(device) > MAX_DEVICE_JSON_BYTES) {
		return failure(400, "invalid_device");
	}
	return success(device);
}

function stringField(
	object: JsonObject,
	keys: readonly string[],
	maxBytes: number,
	error: string,
	required = false,
	nonEmpty = false,
): InputResult<string> {
	const raw = pick(object, keys);
	if (raw === MISSING) {
		return required ? failure(400, error) : success("");
	}
	if (typeof raw !== "string") return failure(400, error);
	const value = truncateUtf8(raw, maxBytes);
	if (nonEmpty && value.trim().length === 0) return failure(400, error);
	return success(value);
}

function idField(
	object: JsonObject,
	keys: readonly string[],
	error: string,
): InputResult<string> {
	const raw = pick(object, keys);
	if (typeof raw !== "string" || !UUID_PATTERN.test(raw)) return failure(400, error);
	return success(raw.toLowerCase());
}

function installIdField(object: JsonObject): InputResult<string> {
	const raw = pick(object, ["installId", "install_id"]);
	if (raw === MISSING || raw === "") return success("unknown");
	if (
		typeof raw !== "string" ||
		raw.trim().length === 0 ||
		/[\u0000-\u001f\u007f]/.test(raw)
	) {
		return failure(400, "invalid_install_id");
	}
	return success(truncateUtf8(raw, 80));
}

function timestampField(object: JsonObject, keys: readonly string[]): InputResult<number> {
	const raw = pick(object, keys);
	if (typeof raw !== "number" || !Number.isSafeInteger(raw) || raw < 0) {
		return failure(400, "invalid_timestamp");
	}
	return success(raw);
}

function booleanField(
	object: JsonObject,
	keys: readonly string[],
	error: string,
	required: true,
): InputResult<boolean>;
function booleanField(
	object: JsonObject,
	keys: readonly string[],
	error: string,
	required: false,
): InputResult<boolean | undefined>;
function booleanField(
	object: JsonObject,
	keys: readonly string[],
	error: string,
	required: boolean,
): InputResult<boolean | undefined> {
	const raw = pick(object, keys);
	if (raw === MISSING) {
		return required ? failure(400, error) : success(undefined);
	}
	return typeof raw === "boolean" ? success(raw) : failure(400, error);
}

function schemaVersion(object: JsonObject): InputResult<1> {
	const raw = pick(object, ["schemaVersion", "schema_version"]);
	if (raw === MISSING || raw === 1) return success(1);
	return failure(400, "unsupported_schema_version");
}

function pick(object: JsonObject, keys: readonly string[]): unknown | typeof MISSING {
	for (const key of keys) {
		if (Object.prototype.hasOwnProperty.call(object, key)) return object[key];
	}
	return MISSING;
}

function isPlainObject(value: unknown): value is JsonObject {
	if (value == null || typeof value !== "object" || Array.isArray(value)) return false;
	const prototype = Object.getPrototypeOf(value);
	return prototype === Object.prototype || prototype === null;
}

function isApplicationJson(contentType: string | null): boolean {
	if (contentType == null) return false;
	const [mediaType, ...parameters] = contentType.split(";");
	if (mediaType?.trim().toLowerCase() !== "application/json") return false;
	for (const parameter of parameters) {
		const separator = parameter.indexOf("=");
		if (separator < 0) continue;
		if (parameter.slice(0, separator).trim().toLowerCase() !== "charset") continue;
		const charset = parameter
			.slice(separator + 1)
			.trim()
			.replace(/^"(.*)"$/, "$1")
			.toLowerCase();
		if (charset !== "utf-8" && charset !== "utf8") return false;
	}
	return true;
}

function joinChunks(chunks: readonly Uint8Array[], totalBytes: number): Uint8Array {
	if (chunks.length === 1) return chunks[0] ?? new Uint8Array();
	const bytes = new Uint8Array(totalBytes);
	let offset = 0;
	for (const chunk of chunks) {
		bytes.set(chunk, offset);
		offset += chunk.byteLength;
	}
	return bytes;
}

function truncateUtf8(value: string, maxBytes: number): string {
	const bytes = UTF8_ENCODER.encode(value);
	if (bytes.byteLength <= maxBytes) return value;
	for (let end = maxBytes; end >= Math.max(0, maxBytes - 3); end -= 1) {
		try {
			return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(
				bytes.subarray(0, end),
			);
		} catch {
			// A UTF-8 boundary is at most three bytes behind the byte cap.
		}
	}
	return "";
}

function jsonBytes(value: unknown): number {
	return UTF8_ENCODER.encode(JSON.stringify(value)).byteLength;
}

function success<T>(value: T): InputResult<T> {
	return { ok: true, value };
}

function failure(status: 400 | 413 | 415, error: string): InputFailure {
	return { ok: false, status, error };
}
