# VniDrop diagnostics API

Cloudflare Worker for ingesting user-submitted bug reports. D1 stores searchable
metadata; R2 stores the larger attached logs.

The service is designed for modest traffic and low operating cost:

- one D1 row is written per bug report;
- bug logs are stored in R2 instead of D1;
- request limits reject oversized work before storage writes;
- an hourly scheduled cleanup and an R2 lifecycle rule enforce retention;
- no Queue, Durable Object, or KV resources are required.

Cloudflare quotas and prices change over time. Check the current
[Workers pricing](https://developers.cloudflare.com/workers/platform/pricing/),
[D1 pricing](https://developers.cloudflare.com/d1/platform/pricing/), and
[R2 pricing](https://developers.cloudflare.com/r2/pricing/) before relying on a
particular free-plan capacity.

## API

All ingest routes require:

```http
X-VniDrop-Key: <INGEST_KEY>
```

The client also sends its anonymous installation ID:

```http
X-VniDrop-Install-Id: <anonymous install UUID>
```

| Method | Path | Body |
|--------|------|------|
| `GET` | `/live` | process liveness; does not touch storage |
| `GET` | `/health` | authenticated readiness; checks required configuration and the D1 schema |
| `POST` | `/v1/bugs` | app bug-report payload |

Report IDs are client-generated UUIDs. A client must reuse the same ID when
retrying so D1 can acknowledge the request without storing it twice.

Accepted reports return `202`. The default is a 262,144-byte request limit.
Cloudflare rate-limit bindings allow 30 requests per
installation and 120 requests per source, per ingest route, per minute. Source
limits run before shared-key verification so rejected traffic is bounded too.
These counters are eventually consistent and local to a Cloudflare location, so
they are abuse mitigation rather than billing or authorization controls.

The API is consumed by native Android, iOS, and desktop clients and does not
enable cross-origin browser access. If a browser-based client is added later,
define a narrow origin allowlist instead of enabling wildcard CORS.

`/health` requires `X-VniDrop-Key` and uses the source limiter because it performs
D1 reads. `/live` is the only unauthenticated probe and never touches storage.

## Security model

`INGEST_KEY` fails closed when it is missing, but it is a shared value embedded in
released app binaries. It can be extracted and therefore is **not** user
authentication, a durable secret, or sufficient abuse protection by itself.

- Store the Worker value with `wrangler secret put`; never put it in
  `wrangler.jsonc`, source control, logs, or command arguments.
- Rotate the key when it is exposed and ship the matching app configuration.
- Keep the two rate-limit namespaces unique within the Cloudflare account. A
  namespace reused by another Worker shares counters with it.
- Use Cloudflare WAF or account-level rate-limiting rules if public abuse exceeds
  what the Worker bindings can absorb.
- Do not log request bodies. Bug reports can contain contact details and attached
  logs.

## Provision and deploy

Run these commands from this directory:

```bash
npm ci
npx wrangler login

npx wrangler d1 create vnidrop-diagnostics
npx wrangler r2 bucket create vnidrop-diagnostics
```

Replace the placeholder `database_id` in `wrangler.jsonc` with the UUID returned
by `wrangler d1 create`. Set the ingest key interactively and configure the R2
retention rule once:

```bash
npx wrangler secret put INGEST_KEY
npx wrangler r2 bucket lifecycle add vnidrop-diagnostics diagnostics-retention --expire-days 90
```

Then apply migrations and deploy from the repository root:

```bash
make diagnostics-db-remote
make deploy-diagnostics
```

`make deploy-diagnostics` runs the complete check before Wrangler changes the
remote Worker.

The lifecycle command changes the remote bucket. Before adding or changing a
rule, inspect the current state with:

```bash
npx wrangler r2 bucket lifecycle list vnidrop-diagnostics
```

## Local development

Create an ignored `.dev.vars` file containing a development-only key:

```dotenv
INGEST_KEY=local-development-only
```

Then initialize the local D1 database and run the Worker:

```bash
# From the repository root:
make diagnostics-db-local
make run-diagnostics
```

Wrangler keeps local D1 and R2 state under the ignored `.wrangler/` directory.
Use `wrangler dev --test-scheduled` when exercising the hourly cleanup handler.

## Migrations and generated types

D1 migrations live in `migrations/` and are recorded in D1's migration ledger.
Never edit an applied migration; add the next numbered SQL file instead.

`worker-configuration.d.ts` is generated from `wrangler.jsonc` and committed so
bindings cannot silently drift from the Worker code:

```bash
make diagnostics-typegen # from the repository root
npm run types:check   # verify the committed file is current
```

Secrets and optional, commented-out bindings are not generated. The source adds
only those narrow extensions to the generated environment type.

Vitest runs inside the Workers runtime. Its setup applies the same numbered D1
migrations to the isolated local database assigned to each test file.

## Retention

`RETENTION_DAYS` defaults to 90. The `17 * * * *` cron trigger runs cleanup at
17 minutes past every hour. Cleanup works in bounded batches: it deletes each
expired report's referenced R2 object before deleting that exact D1 row. The R2
lifecycle rule is an independent backstop for log objects, including objects left
behind by a partial ingest failure. Each scheduled run can remove 7,200 bug rows
while staying below D1's per-invocation query ceiling. Later hourly runs continue
any backlog.
Reaching the cap emits a structured warning with the remaining expired-row count;
alert on that warning because
retention is necessarily best-effort during sustained distributed abuse.

The Worker variable and bucket lifecycle are separate configuration surfaces.
When changing retention, update both `RETENTION_DAYS` and the R2 lifecycle rule;
changing one does not update the other. Cloudflare may delete expired R2 objects
after the exact expiration time rather than synchronously at it.

## App wiring

Keep the tracked root defaults empty. Configure release builds through the
user-level `~/.gradle/gradle.properties` or secured CI Gradle project properties:

```properties
vnidrop.diagnostics.included=true
vnidrop.diagnostics.endpoint=https://vnidrop-diagnostics.<your-subdomain>.workers.dev
vnidrop.diagnostics.ingestKey=<same value as INGEST_KEY>
```

Both the endpoint and key are required. When both are empty the app uses its
offline-safe no-op transport; configuring only one fails the Gradle build.
`vnidrop.diagnostics.included=false` routes bug reports to that no-op transport
(never sent); a configured endpoint accepts an explicit user-submitted bug report.
Treat the app-side key as an abuse-control token with the limitations described
above.

## Reading reports

```bash
npx wrangler d1 execute vnidrop-diagnostics --remote \
  --command "SELECT id, what_happened, status, occurred_at FROM bugs WHERE status = 'open' ORDER BY occurred_at DESC LIMIT 20"
```

R2 object keys use `bugs/<id>/<attempt-id>/logs.txt`. The unique attempt segment
prevents a retry from overwriting an already accepted object before D1 detects
the duplicate.
There is no public administration endpoint; inspect reports through authenticated
Cloudflare tools or a future Access-protected dashboard.
