# :envelope: dataxlr8-email-mcp

Transactional and outreach email for AI agents — send via Resend, manage templates, run multi-step sequences.

[![Rust](https://img.shields.io/badge/Rust-2024_edition-orange?logo=rust)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-rmcp_0.17-blue)](https://modelcontextprotocol.io/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

## What It Does

Sends emails through the Resend API with full template support and multi-step sequence automation. Create reusable HTML templates with variable substitution, enroll contacts into drip sequences, track delivery stats, and maintain complete send history — all through MCP tool calls backed by PostgreSQL.

## Architecture

```
                    ┌─────────────────────────┐
AI Agent ──stdio──▶ │  dataxlr8-email-mcp     │
                    │  (rmcp 0.17 server)      │
                    └─────┬───────────┬───────┘
                          │ sqlx 0.8  │ reqwest 0.12
                          ▼           ▼
                    ┌──────────┐ ┌──────────┐
                    │ PostgreSQL│ │ Resend   │
                    │ schema:  │ │ API      │
                    │ email    │ └──────────┘
                    └──────────┘
```

## Tools

| Tool | Description |
|------|-------------|
| `send_email` | Send a one-off email via Resend |
| `send_template_email` | Send using a saved template with variable substitution |
| `create_template` | Create a reusable HTML email template |
| `list_templates` | List all saved templates |
| `list_sent_emails` | View sent email history with status |
| `email_stats` | Get send, delivery, and bounce statistics |
| `create_sequence` | Create a multi-step outreach sequence |
| `enroll_contact` | Enroll a contact into a sequence |
| `get_sequence_status` | Check enrollment progress for a contact |
| `advance_sequence` | Advance a contact to the next sequence step |
| `pause_enrollment` | Pause a contact's sequence enrollment |
| `list_sequences` | List all sequences |

## Quick Start

```bash
git clone https://github.com/pdaxt/dataxlr8-email-mcp
cd dataxlr8-email-mcp
cargo build --release

export DATABASE_URL=postgres://user:pass@localhost:5432/dataxlr8
export RESEND_API_KEY=re_...
./target/release/dataxlr8-email-mcp
```

The server auto-creates the `email` schema and all tables on first run.

## Configuration

| Variable | Required | Description |
|----------|----------|-------------|
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `RESEND_API_KEY` | Yes | Resend API key for sending |
| `LOG_LEVEL` | No | Tracing level (default: `info`) |

## Claude Desktop Integration

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "dataxlr8-email": {
      "command": "./target/release/dataxlr8-email-mcp",
      "env": {
        "DATABASE_URL": "postgres://user:pass@localhost:5432/dataxlr8",
        "RESEND_API_KEY": "re_..."
      }
    }
  }
}
```

## Part of DataXLR8

One of 14 Rust MCP servers that form the [DataXLR8](https://github.com/pdaxt) platform — a modular, AI-native business operations suite. Each server owns a single domain, shares a PostgreSQL instance, and communicates over the Model Context Protocol.

## License

MIT
