use dataxlr8_mcp_core::mcp::{make_schema, empty_schema, json_result, error_result, get_str, get_i64, get_str_array};
use dataxlr8_mcp_core::Database;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServerHandler;
use serde::{Deserialize, Serialize};
use tracing::info;

// ============================================================================
// Data types
// ============================================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct SentEmail {
    pub id: String,
    pub from_addr: String,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
    pub template: Option<String>,
    pub resend_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct EmailTemplate {
    pub id: String,
    pub name: String,
    pub subject: String,
    pub html_body: String,
    pub from_addr: String,
    pub description: String,
    pub variables: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct EmailStats {
    pub total_sent: i64,
    pub total_failed: i64,
    pub total_dry_run: i64,
    pub recent: Vec<SentEmail>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Sequence {
    pub id: String,
    pub name: String,
    pub steps: serde_json::Value,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct SequenceEnrollment {
    pub id: String,
    pub sequence_id: String,
    pub contact_email: String,
    pub current_step: i32,
    pub status: String,
    pub next_send_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Resend API types
// ============================================================================

#[derive(Debug, Serialize)]
struct ResendRequest {
    from: String,
    to: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cc: Vec<String>,
    subject: String,
    html: String,
}

#[derive(Debug, Deserialize)]
struct ResendResponse {
    id: Option<String>,
}

// ============================================================================
// Tool definitions
// ============================================================================

fn build_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "send_email".into(),
            title: None,
            description: Some("Send an email via Resend API. Logs to database.".into()),
            input_schema: make_schema(serde_json::json!({
                "to": { "type": "array", "items": { "type": "string" }, "description": "Recipient email addresses" },
                "subject": { "type": "string", "description": "Email subject" },
                "html": { "type": "string", "description": "HTML body" },
                "from": { "type": "string", "description": "From address (default: DataXLR8 <noreply@dataxlr8.ai>)" },
                "cc": { "type": "array", "items": { "type": "string" }, "description": "CC addresses" },
                "text": { "type": "string", "description": "Plain text body (fallback)" }
            }), vec!["to", "subject", "html"]),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "send_template_email".into(),
            title: None,
            description: Some("Send an email using a saved template with variable substitution".into()),
            input_schema: make_schema(serde_json::json!({
                "template": { "type": "string", "description": "Template name" },
                "to": { "type": "array", "items": { "type": "string" }, "description": "Recipient emails" },
                "variables": { "type": "object", "description": "Template variables as key-value pairs, e.g. {\"name\": \"John\"}" },
                "cc": { "type": "array", "items": { "type": "string" }, "description": "CC addresses" }
            }), vec!["template", "to"]),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "create_template".into(),
            title: None,
            description: Some("Create or update an email template. Use {{variable}} placeholders.".into()),
            input_schema: make_schema(serde_json::json!({
                "name": { "type": "string", "description": "Unique template name" },
                "subject": { "type": "string", "description": "Subject template (can include {{variables}})" },
                "html_body": { "type": "string", "description": "HTML body template" },
                "from_addr": { "type": "string", "description": "From address" },
                "description": { "type": "string", "description": "What this template is for" },
                "variables": { "type": "array", "items": { "type": "string" }, "description": "List of variable names used" }
            }), vec!["name", "subject", "html_body"]),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "list_templates".into(),
            title: None,
            description: Some("List all email templates".into()),
            input_schema: empty_schema(),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "list_sent_emails".into(),
            title: None,
            description: Some("List sent emails with optional status filter".into()),
            input_schema: make_schema(serde_json::json!({
                "status": { "type": "string", "enum": ["queued", "sent", "failed", "dry_run"], "description": "Filter by status" },
                "limit": { "type": "integer", "description": "Max results (default 50)" }
            }), vec![]),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "email_stats".into(),
            title: None,
            description: Some("Get email sending statistics".into()),
            input_schema: empty_schema(),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "create_sequence".into(),
            title: None,
            description: Some(
                "Create an outreach sequence with multiple steps. Each step has delay_days, subject, html, step_number.".into(),
            ),
            input_schema: make_schema(serde_json::json!({
                "name": { "type": "string", "description": "Unique sequence name" },
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "step_number": { "type": "integer", "description": "Step order (0-based)" },
                            "delay_days": { "type": "integer", "description": "Days to wait before sending this step" },
                            "subject": { "type": "string", "description": "Email subject" },
                            "html": { "type": "string", "description": "HTML body" }
                        },
                        "required": ["step_number", "delay_days", "subject", "html"]
                    },
                    "description": "Array of sequence steps"
                }
            }), vec!["name", "steps"]),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "enroll_contact".into(),
            title: None,
            description: Some("Enroll a contact email into an outreach sequence".into()),
            input_schema: make_schema(serde_json::json!({
                "sequence_id": { "type": "string", "description": "Sequence ID" },
                "contact_email": { "type": "string", "description": "Contact email to enroll" }
            }), vec!["sequence_id", "contact_email"]),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "get_sequence_status".into(),
            title: None,
            description: Some("Get sequence details with all enrolled contacts and their progress".into()),
            input_schema: make_schema(serde_json::json!({
                "sequence_id": { "type": "string", "description": "Sequence ID" }
            }), vec!["sequence_id"]),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "advance_sequence".into(),
            title: None,
            description: Some(
                "Process all active enrollments where next_send_at <= now. Sends the next email in sequence, advances step, sets next_send_at or completes.".into(),
            ),
            input_schema: make_schema(serde_json::json!({
                "sequence_id": { "type": "string", "description": "Optional: only advance enrollments in this sequence" }
            }), vec![]),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "pause_enrollment".into(),
            title: None,
            description: Some("Pause an active enrollment to stop sending".into()),
            input_schema: make_schema(serde_json::json!({
                "enrollment_id": { "type": "string", "description": "Enrollment ID to pause" }
            }), vec!["enrollment_id"]),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
        Tool {
            name: "list_sequences".into(),
            title: None,
            description: Some("List all outreach sequences".into()),
            input_schema: empty_schema(),
            output_schema: None, annotations: None, execution: None, icons: None, meta: None,
        },
    ]
}

// ============================================================================
// MCP Server
// ============================================================================

#[derive(Clone)]
pub struct EmailMcpServer {
    db: Database,
    resend_key: Option<String>,
    http: reqwest::Client,
}

impl EmailMcpServer {
    pub fn new(db: Database, resend_key: Option<String>) -> Self {
        Self {
            db,
            resend_key,
            http: reqwest::Client::new(),
        }
    }

    async fn send_via_resend(
        &self,
        from: &str,
        to: &[String],
        cc: &[String],
        subject: &str,
        html: &str,
    ) -> Result<String, String> {
        let key = match &self.resend_key {
            Some(k) => k,
            None => return Err("dry_run".into()),
        };

        let req = ResendRequest {
            from: from.to_string(),
            to: to.to_vec(),
            cc: cc.to_vec(),
            subject: subject.to_string(),
            html: html.to_string(),
        };

        match self
            .http
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {key}"))
            .json(&req)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    let body: ResendResponse = resp.json().await.map_err(|e| e.to_string())?;
                    Ok(body.id.unwrap_or_default())
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    Err(format!("Resend API {status}: {body}"))
                }
            }
            Err(e) => Err(format!("HTTP error: {e}")),
        }
    }

    // ---- Tool handlers ----

    async fn handle_send_email(&self, args: &serde_json::Value) -> CallToolResult {
        let to = get_str_array(args, "to");
        if to.is_empty() {
            return error_result("Missing required: to (array of emails)");
        }
        let subject = match get_str(args, "subject") {
            Some(s) => s,
            None => return error_result("Missing required: subject"),
        };
        let html = match get_str(args, "html") {
            Some(h) => h,
            None => return error_result("Missing required: html"),
        };
        let from = get_str(args, "from")
            .unwrap_or_else(|| "DataXLR8 <noreply@dataxlr8.ai>".into());
        let cc = get_str_array(args, "cc");
        let text = get_str(args, "text").unwrap_or_default();

        let id = uuid::Uuid::new_v4().to_string();

        // Try to send
        let (status, resend_id, error) = match self.send_via_resend(&from, &to, &cc, &subject, &html).await {
            Ok(rid) => ("sent".to_string(), Some(rid), None),
            Err(e) if e == "dry_run" => ("dry_run".to_string(), None, None),
            Err(e) => ("failed".to_string(), None, Some(e)),
        };

        // Log to database
        match sqlx::query_as::<_, SentEmail>(
            "INSERT INTO email.sent_emails (id, from_addr, to_addrs, cc_addrs, subject, html_body, text_body, resend_id, status, error) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING *",
        )
        .bind(&id)
        .bind(&from)
        .bind(&to)
        .bind(&cc)
        .bind(&subject)
        .bind(&html)
        .bind(&text)
        .bind(&resend_id)
        .bind(&status)
        .bind(&error)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(email) => {
                info!(id = id, status = status, to = ?to, "Email processed");
                json_result(&email)
            }
            Err(e) => error_result(&format!("Failed to log email: {e}")),
        }
    }

    async fn handle_send_template_email(&self, args: &serde_json::Value) -> CallToolResult {
        let template_name = match get_str(args, "template") {
            Some(t) => t,
            None => return error_result("Missing required: template"),
        };
        let to = get_str_array(args, "to");
        if to.is_empty() {
            return error_result("Missing required: to");
        }
        let cc = get_str_array(args, "cc");
        let variables = args.get("variables").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        // Load template
        let tmpl: Option<EmailTemplate> = sqlx::query_as(
            "SELECT * FROM email.templates WHERE name = $1",
        )
        .bind(&template_name)
        .fetch_optional(self.db.pool())
        .await
        .unwrap_or(None);

        let tmpl = match tmpl {
            Some(t) => t,
            None => return error_result(&format!("Template '{template_name}' not found")),
        };

        // Substitute variables
        let mut subject = tmpl.subject.clone();
        let mut html = tmpl.html_body.clone();

        if let Some(obj) = variables.as_object() {
            for (key, val) in obj {
                let placeholder = format!("{{{{{key}}}}}");
                let value = val.as_str().unwrap_or("");
                subject = subject.replace(&placeholder, value);
                html = html.replace(&placeholder, value);
            }
        }

        let id = uuid::Uuid::new_v4().to_string();

        let (status, resend_id, error) = match self.send_via_resend(&tmpl.from_addr, &to, &cc, &subject, &html).await {
            Ok(rid) => ("sent".to_string(), Some(rid), None),
            Err(e) if e == "dry_run" => ("dry_run".to_string(), None, None),
            Err(e) => ("failed".to_string(), None, Some(e)),
        };

        match sqlx::query_as::<_, SentEmail>(
            "INSERT INTO email.sent_emails (id, from_addr, to_addrs, cc_addrs, subject, html_body, template, resend_id, status, error) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING *",
        )
        .bind(&id)
        .bind(&tmpl.from_addr)
        .bind(&to)
        .bind(&cc)
        .bind(&subject)
        .bind(&html)
        .bind(&template_name)
        .bind(&resend_id)
        .bind(&status)
        .bind(&error)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(email) => {
                info!(id = id, template = template_name, status = status, "Template email processed");
                json_result(&email)
            }
            Err(e) => error_result(&format!("Failed to log email: {e}")),
        }
    }

    async fn handle_create_template(&self, args: &serde_json::Value) -> CallToolResult {
        let name = match get_str(args, "name") {
            Some(n) => n,
            None => return error_result("Missing required: name"),
        };
        let subject = match get_str(args, "subject") {
            Some(s) => s,
            None => return error_result("Missing required: subject"),
        };
        let html_body = match get_str(args, "html_body") {
            Some(h) => h,
            None => return error_result("Missing required: html_body"),
        };
        let from_addr = get_str(args, "from_addr")
            .unwrap_or_else(|| "DataXLR8 <noreply@dataxlr8.ai>".into());
        let description = get_str(args, "description").unwrap_or_default();
        let variables = get_str_array(args, "variables");
        let id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, EmailTemplate>(
            "INSERT INTO email.templates (id, name, subject, html_body, from_addr, description, variables) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (name) DO UPDATE SET subject = EXCLUDED.subject, html_body = EXCLUDED.html_body, \
             from_addr = EXCLUDED.from_addr, description = EXCLUDED.description, variables = EXCLUDED.variables, \
             updated_at = now() RETURNING *",
        )
        .bind(&id)
        .bind(&name)
        .bind(&subject)
        .bind(&html_body)
        .bind(&from_addr)
        .bind(&description)
        .bind(&variables)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(tmpl) => {
                info!(name = name, "Created/updated email template");
                json_result(&tmpl)
            }
            Err(e) => error_result(&format!("Failed to create template: {e}")),
        }
    }

    async fn handle_list_templates(&self) -> CallToolResult {
        match sqlx::query_as::<_, EmailTemplate>("SELECT * FROM email.templates ORDER BY name")
            .fetch_all(self.db.pool())
            .await
        {
            Ok(templates) => json_result(&templates),
            Err(e) => error_result(&format!("Database error: {e}")),
        }
    }

    async fn handle_list_sent_emails(&self, args: &serde_json::Value) -> CallToolResult {
        let status = get_str(args, "status");
        let limit = get_i64(args, "limit").unwrap_or(50);

        let (sql, bind) = match &status {
            Some(s) => (
                "SELECT * FROM email.sent_emails WHERE status = $1 ORDER BY created_at DESC LIMIT $2".to_string(),
                Some(s.clone()),
            ),
            None => (
                "SELECT * FROM email.sent_emails ORDER BY created_at DESC LIMIT $1".to_string(),
                None,
            ),
        };

        let result: Result<Vec<SentEmail>, _> = if let Some(ref s) = bind {
            sqlx::query_as(&sql).bind(s).bind(limit).fetch_all(self.db.pool()).await
        } else {
            sqlx::query_as(&sql).bind(limit).fetch_all(self.db.pool()).await
        };

        match result {
            Ok(emails) => json_result(&emails),
            Err(e) => error_result(&format!("Database error: {e}")),
        }
    }

    async fn handle_email_stats(&self) -> CallToolResult {
        let sent: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM email.sent_emails WHERE status = 'sent'")
            .fetch_one(self.db.pool()).await.unwrap_or((0,));
        let failed: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM email.sent_emails WHERE status = 'failed'")
            .fetch_one(self.db.pool()).await.unwrap_or((0,));
        let dry_run: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM email.sent_emails WHERE status = 'dry_run'")
            .fetch_one(self.db.pool()).await.unwrap_or((0,));
        let recent: Vec<SentEmail> = sqlx::query_as(
            "SELECT * FROM email.sent_emails ORDER BY created_at DESC LIMIT 10",
        )
        .fetch_all(self.db.pool()).await.unwrap_or_default();

        json_result(&EmailStats {
            total_sent: sent.0,
            total_failed: failed.0,
            total_dry_run: dry_run.0,
            recent,
        })
    }

    // ---- Sequence handlers ----

    async fn handle_create_sequence(&self, args: &serde_json::Value) -> CallToolResult {
        let name = match get_str(args, "name") {
            Some(n) => n,
            None => return error_result("Missing required: name"),
        };
        let steps = match args.get("steps") {
            Some(s) if s.is_array() => s.clone(),
            _ => return error_result("Missing required: steps (must be a JSON array)"),
        };

        let id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, Sequence>(
            "INSERT INTO email.sequences (id, name, steps) VALUES ($1, $2, $3) RETURNING *",
        )
        .bind(&id)
        .bind(&name)
        .bind(&steps)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(seq) => {
                info!(id = id, name = name, "Created sequence");
                json_result(&seq)
            }
            Err(e) => error_result(&format!("Failed to create sequence: {e}")),
        }
    }

    async fn handle_enroll_contact(&self, args: &serde_json::Value) -> CallToolResult {
        let sequence_id = match get_str(args, "sequence_id") {
            Some(s) => s,
            None => return error_result("Missing required: sequence_id"),
        };
        let contact_email = match get_str(args, "contact_email") {
            Some(e) => e,
            None => return error_result("Missing required: contact_email"),
        };

        // Verify sequence exists and load steps
        let seq: Option<Sequence> = sqlx::query_as(
            "SELECT * FROM email.sequences WHERE id = $1",
        )
        .bind(&sequence_id)
        .fetch_optional(self.db.pool())
        .await
        .unwrap_or(None);

        let seq = match seq {
            Some(s) => s,
            None => return error_result(&format!("Sequence '{sequence_id}' not found")),
        };

        if seq.status != "active" {
            return error_result(&format!("Sequence is {}, not active", seq.status));
        }

        // Calculate first send time from step 0's delay_days
        let first_delay = seq.steps.as_array()
            .and_then(|arr| arr.iter().find(|s| s.get("step_number").and_then(|n| n.as_i64()) == Some(0)))
            .and_then(|s| s.get("delay_days").and_then(|d| d.as_i64()))
            .unwrap_or(0);

        let next_send_at = chrono::Utc::now() + chrono::Duration::days(first_delay);
        let id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, SequenceEnrollment>(
            "INSERT INTO email.sequence_enrollments (id, sequence_id, contact_email, current_step, next_send_at) \
             VALUES ($1, $2, $3, 0, $4) RETURNING *",
        )
        .bind(&id)
        .bind(&sequence_id)
        .bind(&contact_email)
        .bind(next_send_at)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(enrollment) => {
                info!(sequence_id = sequence_id, email = contact_email, "Enrolled contact");
                json_result(&enrollment)
            }
            Err(e) => {
                if e.to_string().contains("unique") || e.to_string().contains("duplicate") {
                    error_result(&format!("Contact '{contact_email}' already enrolled in this sequence"))
                } else {
                    error_result(&format!("Failed to enroll contact: {e}"))
                }
            }
        }
    }

    async fn handle_get_sequence_status(&self, args: &serde_json::Value) -> CallToolResult {
        let sequence_id = match get_str(args, "sequence_id") {
            Some(s) => s,
            None => return error_result("Missing required: sequence_id"),
        };

        let seq: Option<Sequence> = sqlx::query_as(
            "SELECT * FROM email.sequences WHERE id = $1",
        )
        .bind(&sequence_id)
        .fetch_optional(self.db.pool())
        .await
        .unwrap_or(None);

        let seq = match seq {
            Some(s) => s,
            None => return error_result(&format!("Sequence '{sequence_id}' not found")),
        };

        let enrollments: Vec<SequenceEnrollment> = sqlx::query_as(
            "SELECT * FROM email.sequence_enrollments WHERE sequence_id = $1 ORDER BY created_at",
        )
        .bind(&sequence_id)
        .fetch_all(self.db.pool())
        .await
        .unwrap_or_default();

        json_result(&serde_json::json!({
            "sequence": seq,
            "enrollments": enrollments,
            "total_enrolled": enrollments.len(),
            "active": enrollments.iter().filter(|e| e.status == "active").count(),
            "completed": enrollments.iter().filter(|e| e.status == "completed").count(),
            "paused": enrollments.iter().filter(|e| e.status == "paused").count(),
        }))
    }

    async fn handle_advance_sequence(&self, args: &serde_json::Value) -> CallToolResult {
        let sequence_filter = get_str(args, "sequence_id");

        // Find all active enrollments ready to send
        let enrollments: Vec<SequenceEnrollment> = if let Some(ref sid) = sequence_filter {
            sqlx::query_as(
                "SELECT * FROM email.sequence_enrollments \
                 WHERE status = 'active' AND next_send_at <= now() AND sequence_id = $1",
            )
            .bind(sid)
            .fetch_all(self.db.pool())
            .await
            .unwrap_or_default()
        } else {
            sqlx::query_as(
                "SELECT * FROM email.sequence_enrollments \
                 WHERE status = 'active' AND next_send_at <= now()",
            )
            .fetch_all(self.db.pool())
            .await
            .unwrap_or_default()
        };

        let mut sent_count = 0i64;
        let mut completed_count = 0i64;
        let mut failed_count = 0i64;
        let mut errors: Vec<String> = Vec::new();

        for enrollment in &enrollments {
            // Load sequence
            let seq: Option<Sequence> = sqlx::query_as(
                "SELECT * FROM email.sequences WHERE id = $1",
            )
            .bind(&enrollment.sequence_id)
            .fetch_optional(self.db.pool())
            .await
            .unwrap_or(None);

            let seq = match seq {
                Some(s) => s,
                None => {
                    errors.push(format!("{}: sequence not found", enrollment.id));
                    continue;
                }
            };

            // Find current step
            let steps = match seq.steps.as_array() {
                Some(a) => a,
                None => {
                    errors.push(format!("{}: invalid steps JSON", enrollment.id));
                    continue;
                }
            };

            let step = steps.iter().find(|s| {
                s.get("step_number").and_then(|n| n.as_i64()) == Some(enrollment.current_step as i64)
            });

            let step = match step {
                Some(s) => s,
                None => {
                    // No more steps — mark completed
                    let _ = sqlx::query(
                        "UPDATE email.sequence_enrollments SET status = 'completed', next_send_at = NULL WHERE id = $1",
                    )
                    .bind(&enrollment.id)
                    .execute(self.db.pool())
                    .await;
                    completed_count += 1;
                    continue;
                }
            };

            let subject = step.get("subject").and_then(|v| v.as_str()).unwrap_or("(no subject)");
            let html = step.get("html").and_then(|v| v.as_str()).unwrap_or("");
            let from = "DataXLR8 <noreply@dataxlr8.ai>";

            // Send email
            let to = vec![enrollment.contact_email.clone()];
            let (status, resend_id, error) = match self.send_via_resend(from, &to, &[], subject, html).await {
                Ok(rid) => ("sent".to_string(), Some(rid), None),
                Err(e) if e == "dry_run" => ("dry_run".to_string(), None, None),
                Err(e) => ("failed".to_string(), None, Some(e)),
            };

            // Log sent email
            let email_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO email.sent_emails (id, from_addr, to_addrs, cc_addrs, subject, html_body, resend_id, status, error, metadata) \
                 VALUES ($1, $2, $3, '{}', $4, $5, $6, $7, $8, $9)",
            )
            .bind(&email_id)
            .bind(from)
            .bind(&to)
            .bind(subject)
            .bind(html)
            .bind(&resend_id)
            .bind(&status)
            .bind(&error)
            .bind(serde_json::json!({"sequence_id": enrollment.sequence_id, "enrollment_id": enrollment.id, "step": enrollment.current_step}))
            .execute(self.db.pool())
            .await;

            if status == "failed" {
                failed_count += 1;
                errors.push(format!("{}: send failed — {}", enrollment.contact_email, error.unwrap_or_default()));
                continue;
            }

            sent_count += 1;

            // Advance to next step
            let next_step = enrollment.current_step + 1;
            let next_step_data = steps.iter().find(|s| {
                s.get("step_number").and_then(|n| n.as_i64()) == Some(next_step as i64)
            });

            if let Some(ns) = next_step_data {
                let delay = ns.get("delay_days").and_then(|d| d.as_i64()).unwrap_or(1);
                let next_send = chrono::Utc::now() + chrono::Duration::days(delay);
                let _ = sqlx::query(
                    "UPDATE email.sequence_enrollments SET current_step = $1, next_send_at = $2 WHERE id = $3",
                )
                .bind(next_step)
                .bind(next_send)
                .bind(&enrollment.id)
                .execute(self.db.pool())
                .await;
            } else {
                // No more steps
                let _ = sqlx::query(
                    "UPDATE email.sequence_enrollments SET current_step = $1, status = 'completed', next_send_at = NULL WHERE id = $2",
                )
                .bind(next_step)
                .bind(&enrollment.id)
                .execute(self.db.pool())
                .await;
                completed_count += 1;
            }
        }

        json_result(&serde_json::json!({
            "processed": enrollments.len(),
            "sent": sent_count,
            "completed": completed_count,
            "failed": failed_count,
            "errors": errors
        }))
    }

    async fn handle_pause_enrollment(&self, args: &serde_json::Value) -> CallToolResult {
        let enrollment_id = match get_str(args, "enrollment_id") {
            Some(e) => e,
            None => return error_result("Missing required: enrollment_id"),
        };

        match sqlx::query_as::<_, SequenceEnrollment>(
            "UPDATE email.sequence_enrollments SET status = 'paused', next_send_at = NULL \
             WHERE id = $1 AND status = 'active' RETURNING *",
        )
        .bind(&enrollment_id)
        .fetch_optional(self.db.pool())
        .await
        {
            Ok(Some(e)) => {
                info!(id = enrollment_id, "Paused enrollment");
                json_result(&e)
            }
            Ok(None) => error_result(&format!("Enrollment '{enrollment_id}' not found or not active")),
            Err(e) => error_result(&format!("Failed to pause enrollment: {e}")),
        }
    }

    async fn handle_list_sequences(&self) -> CallToolResult {
        match sqlx::query_as::<_, Sequence>(
            "SELECT * FROM email.sequences ORDER BY created_at DESC",
        )
        .fetch_all(self.db.pool())
        .await
        {
            Ok(seqs) => json_result(&seqs),
            Err(e) => error_result(&format!("Database error: {e}")),
        }
    }
}

// ============================================================================
// ServerHandler
// ============================================================================

impl ServerHandler for EmailMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "DataXLR8 Email MCP — send emails via Resend, manage templates, track delivery".into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::ErrorData>> + Send + '_ {
        async {
            Ok(ListToolsResult {
                tools: build_tools(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>> + Send + '_ {
        async move {
            let args = serde_json::to_value(&request.arguments).unwrap_or(serde_json::Value::Null);
            let name_str: &str = request.name.as_ref();

            let result = match name_str {
                "send_email" => self.handle_send_email(&args).await,
                "send_template_email" => self.handle_send_template_email(&args).await,
                "create_template" => self.handle_create_template(&args).await,
                "list_templates" => self.handle_list_templates().await,
                "list_sent_emails" => self.handle_list_sent_emails(&args).await,
                "email_stats" => self.handle_email_stats().await,
                "create_sequence" => self.handle_create_sequence(&args).await,
                "enroll_contact" => self.handle_enroll_contact(&args).await,
                "get_sequence_status" => self.handle_get_sequence_status(&args).await,
                "advance_sequence" => self.handle_advance_sequence(&args).await,
                "pause_enrollment" => self.handle_pause_enrollment(&args).await,
                "list_sequences" => self.handle_list_sequences().await,
                _ => error_result(&format!("Unknown tool: {}", request.name)),
            };

            Ok(result)
        }
    }
}
