# VniDrop Core Send/Receive Flow

This crate owns the transfer backend. Platform/UI code should pass file handles
or paths into Rust and react to `CoreEvent` updates; it should not move file
bytes through Kotlin memory.

## Send

1. `initialize(app_data_dir, event_sink)` starts the Iroh endpoint, blob
   provider, handshake protocol, SQLite repository, and event hub.
2. `share_files(sources, metadata)` validates platform sources, streams each
   file into `iroh-blobs`, stores a collection, and returns a VniDrop ticket.
3. New VniDrop shares are `ApprovalRequired` by default. A copied ticket is not
   enough to read bytes until the sender approves the receiver endpoint.
4. The blob provider is **default-deny**: only hashes registered for an active
   share (collection root **and** each member blob) may be served, and only when
   access policy allows that remote endpoint. Unknown hashes are refused.
5. The sender observes receiver requests through `CoreEvent` entries with
   `phase="approval"` and can query them with
   `list_receiver_requests(transfer_id)`.
6. `respond_receiver_request(request_id, accepted, reason)` accepts or refuses a
   pending request. Accepted requests create a time-limited access session for
   the receiver endpoint.
7. Ticket strings are capabilities. Share events emit hash/size metadata only —
   never the full ticket payload.

## Receive

1. `receive(ticket, output_dir, receiver_name)` parses and validates the ticket.
2. VniDrop tickets first connect to the handshake ALPN
   `/vnidrop/handshake/2` and send `RequestTransfer` metadata to the sender.
3. If approved, the receiver connects to the blobs ALPN, downloads the
   collection, and streams files to `output_dir`.
4. If refused, expired, unknown, or cancelled, the receive transfer is marked
   `failed` or `cancelled` and emits an error/lifecycle event.
5. Only `vnd1:` VniDrop tickets are accepted. Raw iroh `BlobTicket` strings are
   rejected at parse time so receive always runs the approval handshake.

## Core States And Events

- Transfer statuses: `sharing`, `receiving`, `done`, `failed`, `cancelled`,
  `stopped`.
- Main event phases: `endpoint`, `import`, `ticket`, `handshake`, `approval`,
  `access`, `transfer`, `download`, `export`, `delivery`, `lifecycle`, `error`.
- Events are sent to `CoreEventSink` immediately and persisted through the event
  hub. `list_events` flushes queued persistence before reading SQLite.
- `shutdown()` is idempotent and flushes events before stopping the router.

## Platform File Rules

- Desktop uses normal filesystem paths.
- Android opens SAF/content URIs in Kotlin and passes a borrowed file
  descriptor; Rust duplicates the descriptor before streaming.
- iOS starts the security-scoped URL lease in Swift and keeps it alive while
  Rust streams from the accessible file URL/path.

## Durability And Filesystem Policy

- SQLite records have a local UUID in addition to the protocol transfer ID.
  Schema-v2 records are migrated in place and keep their tickets and history.
- Imports and receives are recorded before work begins. A process restart marks
  interrupted work failed and expires approval requests that no longer have an
  in-memory responder.
- Persisted shares are restored only when their root collection is complete and
  readable. Missing or corrupt roots fail closed and emit a recovery event.
- Receive destinations use a no-overwrite policy. Rust writes a uniquely named
  temporary file in the destination directory, syncs it, and publishes it with
  a no-clobber hard link when the filesystem supports it. On platforms that
  reject hard links (notably Android emulated external storage), publication
  falls back to an exclusive rename (`renameat2(RENAME_NOREPLACE)` /
  `renamex_np(RENAME_EXCL)`). Failure or cancellation removes the temporary
  file. Stale VniDrop temporary files are cleaned on later writes.
- Android defaults to the shared system Downloads collection via MediaStore
  (`ReceiveFolderKind.AndroidPublicDownloads` on API 29+). Files show up in the
  user's Downloads UI like a browser download. Custom folders still use a SAF
  tree URI from the folder picker. Both Android sinks stream through
  `ReceiveOutputSink` instead of raw filesystem paths. Pre-Android 10 falls
  back to the public Downloads path with legacy storage permission.
- Foreign output sinks receive exactly one terminal callback after a successful
  `start_file`: `finish_file` or `abort_file`.

## Blob Retention Policy

Stopping a share immediately removes its provider mapping and approval state,
so outstanding VniDrop tickets can no longer download content. Physical blob
chunks are not force-deleted at stop time because content-addressed chunks may be
shared by another active collection. Every active outgoing share has a persistent
`vnidrop/share/<local-id>` tag; stopping or deleting it removes that tag, and the
configured garbage collector later reclaims content with no remaining persistent
or temporary tag. Receive downloads keep a temporary tag through export and become
reclaimable after publication. Restart reconciliation repairs active-share tags,
removes orphan share tags, and never restores a stopped share.

The explicit transfer-cache action is available only when no transfer or share is
active. It shuts the core down cleanly, removes the app-owned blob store, and then
restarts the core with the same identity and network configuration. Deleting all
transfer records invokes the same cleanup after live shares have been stopped.

## Resource Limits

`CoreLimits` controls source count, collection files and bytes, path and ticket
sizes, metadata, retained events, pending approvals, concurrent transfers, and
the event persistence queue. `initialize` uses conservative defaults (including
bounded ticket size, pending approvals, and total collection bytes);
`initialize_with_limits` supports stricter deployments and tests. Cheap limits
are checked before durable or network work, while remote collection limits are
checked before downloading file content.

Manual `approve_endpoint_for_transfer` only applies to active shares, requires a
non-empty endpoint id, and creates a time-limited session (same TTL as handshake
approval), never a permanent grant.
