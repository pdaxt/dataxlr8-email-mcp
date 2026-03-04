use dataxlr8_mcp_core::Database;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServerHandler;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
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
// Tool schema helpers
// ============================================================================

fn make_schema(
    properties: serde_json::Value,
    required: Vec<&str>,
) -> Arc<serde_json::Map<String, serde_json::Value>> {
    let mut m = serde_json::Map::new();
    m.insert("type".into(), serde_json::Value::String("object".into()));
    m.insert("properties".into(), properties);
    if !required.is_empty() {
        m.insert(
            "required".into(),
            serde_json::Value::Array(
                required.into_iter().map(|s| serde_json::Value::String(s.into())).collect(),
            ),
        );
    }
    Arc::new(m)
}

fn empty_schema() -> Arc<serde_json::Map<String, serde_json::Value>> {
    let mut m = serde_json::Map::new();
    m.insert("type".into(), serde_json::Value::String("object".into()));
    Arc::new(m)
}

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

    fn json_result<T: Serialize>(data: &T) -> CallToolResult {
        match serde_json::to_string_pretty(data) {
            Ok(json) => CallToolResult::success(vec![Content::text(json)]),
            Err(e) => CallToolResult::error(vec![Content::text(format!("Serialization error: {e}"))]),
        }
    }

    fn error_result(msg: &str) -> CallToolResult {
        CallToolResult::error(vec![Content::text(msg.to_string())])
    }

    fn get_str(args: &serde_json::Value, key: &str) -> Option<String> {
        args.get(key).and_then(|v| v.as_str()).map(String::from)
    }

    fn get_i64(args: &serde_json::Value, key: &str) -> Option<i64> {
        args.get(key).and_then(|v| v.as_i64())
    }

    fn get_str_array(args: &serde_json::Value, key: &str) -> Vec<String> {
        args.get(key)
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
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
        let to = Self::get_str_array(args, "to");
        if to.is_empty() {
            return Self::error_result("Missing required: to (array of emails)");
        }
        let subject = match Self::get_str(args, "subject") {
            Some(s) => s,
            None => return Self::error_result("Missing required: subject"),
        };
        let html = match Self::get_str(args, "html") {
            Some(h) => h,
            None => return Self::error_result("Missing required: html"),
        };
        let from = Self::get_str(args, "from")
            .unwrap_or_else(|| "DataXLR8 <noreply@dataxlr8.ai>".into());
        let cc = Self::get_str_array(args, "cc");
        let text = Self::get_str(args, "text").unwrap_or_default();

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
                Self::json_result(&email)
            }
            Err(e) => Self::error_result(&format!("Failed to log email: {e}")),
        }
    }

    async fn handle_send_template_email(&self, args: &serde_json::Value) -> CallToolResult {
        let template_name = match Self::get_str(args, "template") {
            Some(t) => t,
            None => return Self::error_result("Missing required: template"),
        };
        let to = Self::get_str_array(args, "to");
        if to.is_empty() {
            return Self::error_result("Missing required: to");
        }
        let cc = Self::get_str_array(args, "cc");
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
            None => return Self::error_result(&format!("Template '{template_name}' not found")),
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
                Self::json_result(&email)
            }
            Err(e) => Self::error_result(&format!("Failed to log email: {e}")),
        }
    }

    async fn handle_create_template(&self, args: &serde_json::Value) -> CallToolResult {
        let name = match Self::get_str(args, "name") {
            Some(n) => n,
            None => return Self::error_result("Missing required: name"),
        };
        let subject = match Self::get_str(args, "subject") {
            Some(s) => s,
            None => return Self::error_result("Missing required: subject"),
        };
        let html_body = match Self::get_str(args, "html_body") {
            Some(h) => h,
            None => return Self::error_result("Missing required: html_body"),
        };
        let from_addr = Self::get_str(args, "from_addr")
            .unwrap_or_else(|| "DataXLR8 <noreply@dataxlr8.ai>".into());
        let description = Self::get_str(args, "description").unwrap_or_default();
        let variables = Self::get_str_array(args, "variables");
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
                Self::json_result(&tmpl)
            }
            Err(e) => Self::error_result(&format!("Failed to create template: {e}")),
        }
    }

    async fn handle_list_templates(&self) -> CallToolResult {
        match sqlx::query_as::<_, EmailTemplate>("SELECT * FROM email.templates ORDER BY name")
            .fetch_all(self.db.pool())
            .await
        {
            Ok(templates) => Self::json_result(&templates),
            Err(e) => Self::error_result(&format!("Database error: {e}")),
        }
    }

    async fn handle_list_sent_emails(&self, args: &serde_json::Value) -> CallToolResult {
        let status = Self::get_str(args, "status");
        let limit = Self::get_i64(args, "limit").unwrap_or(50);

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
            Ok(emails) => Self::json_result(&emails),
            Err(e) => Self::error_result(&format!("Database error: {e}")),
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

        Self::json_result(&EmailStats {
            total_sent: sent.0,
            total_failed: failed.0,
            total_dry_run: dry_run.0,
            recent,
        })
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
                _ => Self::error_result(&format!("Unknown tool: {}", request.name)),
            };

            Ok(result)
        }
    }
}
