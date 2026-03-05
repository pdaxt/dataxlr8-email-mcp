use anyhow::Result;
use sqlx::PgPool;

pub async fn setup_schema(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        r#"
        CREATE SCHEMA IF NOT EXISTS email;

        CREATE TABLE IF NOT EXISTS email.sent_emails (
            id          TEXT PRIMARY KEY,
            from_addr   TEXT NOT NULL,
            to_addrs    TEXT[] NOT NULL,
            cc_addrs    TEXT[] NOT NULL DEFAULT '{}',
            subject     TEXT NOT NULL,
            html_body   TEXT NOT NULL DEFAULT '',
            text_body   TEXT NOT NULL DEFAULT '',
            template    TEXT,
            resend_id   TEXT,
            status      TEXT NOT NULL DEFAULT 'queued'
                        CHECK (status IN ('queued', 'sent', 'failed', 'dry_run')),
            error       TEXT,
            metadata    JSONB NOT NULL DEFAULT '{}',
            created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE IF NOT EXISTS email.templates (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            subject     TEXT NOT NULL,
            html_body   TEXT NOT NULL,
            from_addr   TEXT NOT NULL DEFAULT 'DataXLR8 <noreply@dataxlr8.ai>',
            description TEXT NOT NULL DEFAULT '',
            variables   TEXT[] NOT NULL DEFAULT '{}',
            created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE IF NOT EXISTS email.sequences (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            steps       JSONB NOT NULL DEFAULT '[]',
            status      TEXT NOT NULL DEFAULT 'active'
                        CHECK (status IN ('active', 'paused', 'archived')),
            created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE IF NOT EXISTS email.sequence_enrollments (
            id              TEXT PRIMARY KEY,
            sequence_id     TEXT NOT NULL REFERENCES email.sequences(id) ON DELETE CASCADE,
            contact_email   TEXT NOT NULL,
            current_step    INTEGER NOT NULL DEFAULT 0,
            status          TEXT NOT NULL DEFAULT 'active'
                            CHECK (status IN ('active', 'paused', 'completed', 'bounced')),
            next_send_at    TIMESTAMPTZ,
            created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE (sequence_id, contact_email)
        );

        CREATE INDEX IF NOT EXISTS idx_sent_emails_status ON email.sent_emails(status);
        CREATE INDEX IF NOT EXISTS idx_sent_emails_created ON email.sent_emails(created_at);
        CREATE INDEX IF NOT EXISTS idx_templates_name ON email.templates(name);
        CREATE INDEX IF NOT EXISTS idx_sequences_status ON email.sequences(status);
        CREATE INDEX IF NOT EXISTS idx_enrollments_sequence ON email.sequence_enrollments(sequence_id);
        CREATE INDEX IF NOT EXISTS idx_enrollments_next_send ON email.sequence_enrollments(next_send_at)
            WHERE status = 'active';
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
