# dataxlr8-email-mcp

Email MCP server for the DataXLR8 platform — Resend API, templates, sequences, and logging.

## What It Does

Sends transactional and outreach emails via the Resend API. Supports reusable HTML templates with variables, multi-step email sequences with enrollment tracking, and full send logging. All email history and templates are persisted in PostgreSQL.

## Tools

| Tool | Description |
|------|-------------|
| `send_email` | Send a one-off email via Resend |
| `send_template_email` | Send an email using a saved template |
| `create_template` | Create a reusable email template with variables |
| `list_templates` | List all saved templates |
| `list_sent_emails` | View sent email history |
| `email_stats` | Get send/delivery/bounce statistics |
| `create_sequence` | Create a multi-step outreach sequence |
| `enroll_contact` | Enroll a contact into a sequence |
| `get_sequence_status` | Check enrollment status for a contact |
| `advance_sequence` | Advance a contact to the next sequence step |
| `pause_enrollment` | Pause a contact's sequence enrollment |
| `list_sequences` | List all sequences |

## Quick Start

```bash
export DATABASE_URL=postgres://user:pass@localhost:5432/dataxlr8
export RESEND_API_KEY=re_...

cargo build
cargo run
```

## Schema

Creates an `email` schema with:

| Table | Purpose |
|-------|---------|
| `email.sent_emails` | Log of all sent emails (to, from, subject, status) |
| `email.templates` | Reusable email templates with HTML body and variables |
| `email.sequences` | Multi-step outreach sequence definitions |
| `email.sequence_enrollments` | Per-contact enrollment state and scheduling |

## Part of the [DataXLR8](https://github.com/pdaxt) Platform
