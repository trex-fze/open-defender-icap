ALTER TABLE iam_audit_events
    ADD CONSTRAINT iam_audit_payload_object CHECK (
        payload IS NULL OR jsonb_typeof(payload) = 'object'
    );

CREATE INDEX IF NOT EXISTS iam_audit_events_actor_idx ON iam_audit_events (actor);
