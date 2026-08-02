-- Telemetry and crash auto-reporting were removed from the app; only user-initiated
-- bug reports remain. Drop the now-unused ingestion tables and their indexes.
DROP INDEX IF EXISTS idx_event_batches_received;
DROP INDEX IF EXISTS idx_event_batches_install;
DROP TABLE IF EXISTS event_batches;

DROP INDEX IF EXISTS idx_crashes_received;
DROP INDEX IF EXISTS idx_crashes_fingerprint;
DROP INDEX IF EXISTS idx_crashes_install;
DROP TABLE IF EXISTS crashes;
