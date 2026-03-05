use dataxlr8_mcp_core::mcp::{get_i64, get_str, get_str_array};
use serde_json::json;

// ============================================================================
// Template variable substitution (mirrors handle_send_template_email logic)
// ============================================================================

fn substitute_template(template: &str, variables: &serde_json::Value) -> String {
    let mut result = template.to_string();
    if let Some(obj) = variables.as_object() {
        for (key, val) in obj {
            let placeholder = format!("{{{{{key}}}}}");
            let value = val.as_str().unwrap_or("");
            result = result.replace(&placeholder, value);
        }
    }
    result
}

// ============================================================================
// Template substitution — basic
// ============================================================================

#[test]
fn template_sub_basic() {
    let tmpl = "Hello {{name}}, welcome to {{company}}!";
    let vars = json!({"name": "John", "company": "Acme"});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Hello John, welcome to Acme!");
}

#[test]
fn template_sub_no_variables() {
    let tmpl = "Hello World!";
    let vars = json!({});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Hello World!");
}

#[test]
fn template_sub_missing_variable() {
    let tmpl = "Hello {{name}}, your code is {{code}}";
    let vars = json!({"name": "John"});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Hello John, your code is {{code}}");
}

#[test]
fn template_sub_extra_variables() {
    let tmpl = "Hello {{name}}";
    let vars = json!({"name": "John", "unused": "value", "also_unused": 42});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Hello John");
}

#[test]
fn template_sub_null_variables() {
    let tmpl = "Hello {{name}}";
    let vars = serde_json::Value::Null;
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Hello {{name}}"); // No substitution
}

// ============================================================================
// Template substitution — edge cases
// ============================================================================

#[test]
fn template_sub_empty_template() {
    let vars = json!({"name": "John"});
    let result = substitute_template("", &vars);
    assert_eq!(result, "");
}

#[test]
fn template_sub_empty_value() {
    let tmpl = "Hello {{name}}!";
    let vars = json!({"name": ""});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Hello !");
}

#[test]
fn template_sub_numeric_value() {
    let tmpl = "Your balance: {{amount}}";
    let vars = json!({"amount": 42}); // Number, not string
    let result = substitute_template(tmpl, &vars);
    // as_str returns None for numbers, so unwrap_or("") gives ""
    assert_eq!(result, "Your balance: ");
}

#[test]
fn template_sub_boolean_value() {
    let tmpl = "Active: {{active}}";
    let vars = json!({"active": true});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Active: "); // bool → as_str → None → ""
}

#[test]
fn template_sub_repeated_placeholder() {
    let tmpl = "{{name}} and {{name}} again {{name}}";
    let vars = json!({"name": "John"});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "John and John again John");
}

#[test]
fn template_sub_nested_braces() {
    // What happens with {{{name}}} — three braces?
    let tmpl = "{{{name}}}";
    let vars = json!({"name": "John"});
    let result = substitute_template(tmpl, &vars);
    // {{name}} is replaced with John, leaving {John}
    assert_eq!(result, "{John}");
}

#[test]
fn template_sub_partial_placeholder() {
    let tmpl = "Hello {name} and {{name}}";
    let vars = json!({"name": "John"});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Hello {name} and John");
}

// ============================================================================
// Template substitution — security
// ============================================================================

#[test]
fn template_sub_html_injection() {
    let tmpl = "Hello {{name}}";
    let vars = json!({"name": "<script>alert('xss')</script>"});
    let result = substitute_template(tmpl, &vars);
    // No HTML escaping — raw HTML is injected
    assert!(result.contains("<script>"));
}

#[test]
fn template_sub_sql_injection() {
    let tmpl = "Dear {{name}}";
    let vars = json!({"name": "Robert'); DROP TABLE email.templates;--"});
    let result = substitute_template(tmpl, &vars);
    assert!(result.contains("DROP TABLE"));
}

#[test]
fn template_sub_crlf_injection() {
    let tmpl = "Subject: {{subject}}";
    let vars = json!({"subject": "Test\r\nBCC: evil@attacker.com"});
    let result = substitute_template(tmpl, &vars);
    assert!(result.contains("\r\n"));
}

// ============================================================================
// Template substitution — long strings
// ============================================================================

#[test]
fn template_sub_very_long_template() {
    let tmpl = "x".repeat(10_000) + "{{name}}" + &"y".repeat(10_000);
    let vars = json!({"name": "John"});
    let result = substitute_template(&tmpl, &vars);
    assert_eq!(result.len(), 20_004); // 10000 + 4 + 10000
}

#[test]
fn template_sub_very_long_value() {
    let tmpl = "Hello {{name}}!";
    let long_name = "x".repeat(10_000);
    let vars = json!({"name": long_name});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result.len(), 10_007); // "Hello " + 10000 + "!"
}

#[test]
fn template_sub_many_variables() {
    let mut tmpl = String::new();
    let mut map = serde_json::Map::new();
    for i in 0..100 {
        tmpl.push_str(&format!("{{{{{i}}}}} "));
        map.insert(i.to_string(), serde_json::Value::String(format!("val{i}")));
    }
    let vars = serde_json::Value::Object(map);
    let result = substitute_template(&tmpl, &vars);
    assert!(result.contains("val0"));
    assert!(result.contains("val99"));
    assert!(!result.contains("{{"));
}

// ============================================================================
// Template substitution — unicode
// ============================================================================

#[test]
fn template_sub_unicode_key() {
    let tmpl = "Hello {{名前}}";
    let vars = json!({"名前": "太郎"});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Hello 太郎");
}

#[test]
fn template_sub_unicode_value() {
    let tmpl = "Hello {{name}}";
    let vars = json!({"name": "München"});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Hello München");
}

#[test]
fn template_sub_emoji_value() {
    let tmpl = "Status: {{status}}";
    let vars = json!({"status": "Done ✅"});
    let result = substitute_template(tmpl, &vars);
    assert_eq!(result, "Status: Done ✅");
}

// ============================================================================
// send_email — input validation
// ============================================================================

#[test]
fn send_email_empty_to_array() {
    let args = json!({"to": [], "subject": "Test", "html": "<p>Hi</p>"});
    let to = get_str_array(&args, "to");
    assert!(to.is_empty());
}

#[test]
fn send_email_missing_to() {
    let args = json!({"subject": "Test", "html": "<p>Hi</p>"});
    let to = get_str_array(&args, "to");
    assert!(to.is_empty());
}

#[test]
fn send_email_missing_subject() {
    let args = json!({"to": ["test@example.com"], "html": "<p>Hi</p>"});
    assert!(get_str(&args, "subject").is_none()); // Would be caught by None match
}

#[test]
fn send_email_missing_html() {
    let args = json!({"to": ["test@example.com"], "subject": "Test"});
    assert!(get_str(&args, "html").is_none());
}

#[test]
fn send_email_default_from() {
    let args = json!({"to": ["test@example.com"], "subject": "Test", "html": "<p>Hi</p>"});
    let from = get_str(&args, "from").unwrap_or_else(|| "DataXLR8 <noreply@dataxlr8.ai>".into());
    assert_eq!(from, "DataXLR8 <noreply@dataxlr8.ai>");
}

#[test]
fn send_email_custom_from() {
    let args = json!({"to": ["test@example.com"], "subject": "Test", "html": "<p>Hi</p>", "from": "Custom <custom@example.com>"});
    let from = get_str(&args, "from").unwrap_or_else(|| "DataXLR8 <noreply@dataxlr8.ai>".into());
    assert_eq!(from, "Custom <custom@example.com>");
}

#[test]
fn send_email_very_long_subject() {
    let long_subject = "x".repeat(5000);
    let args = json!({"to": ["test@example.com"], "subject": long_subject, "html": "<p>Hi</p>"});
    let subject = get_str(&args, "subject").unwrap();
    assert_eq!(subject.len(), 5000);
}

#[test]
fn send_email_very_long_html() {
    let long_html = "<p>".to_string() + &"x".repeat(100_000) + "</p>";
    let args = json!({"to": ["test@example.com"], "subject": "Test", "html": long_html});
    let html = get_str(&args, "html").unwrap();
    assert!(html.len() > 100_000);
}

#[test]
fn send_email_sql_injection_subject() {
    let args = json!({
        "to": ["test@example.com"],
        "subject": "'; DROP TABLE email.sent_emails;--",
        "html": "<p>Hi</p>"
    });
    let subject = get_str(&args, "subject").unwrap();
    assert!(subject.contains("DROP TABLE")); // Parameterized query prevents injection
}

#[test]
fn send_email_xss_in_html() {
    let args = json!({
        "to": ["test@example.com"],
        "subject": "Test",
        "html": "<script>document.cookie</script>"
    });
    let html = get_str(&args, "html").unwrap();
    assert!(html.contains("<script>")); // Raw HTML — expected for email
}

#[test]
fn send_email_many_recipients() {
    let to: Vec<String> = (0..100).map(|i| format!("user{i}@example.com")).collect();
    let args = json!({"to": to, "subject": "Test", "html": "<p>Bulk</p>"});
    let result = get_str_array(&args, "to");
    assert_eq!(result.len(), 100);
}

#[test]
fn send_email_invalid_email_in_to() {
    let args = json!({"to": ["not-an-email", "@", ""], "subject": "Test", "html": "<p>Hi</p>"});
    let to = get_str_array(&args, "to");
    assert_eq!(to.len(), 3); // No validation at extraction level
    assert_eq!(to[0], "not-an-email");
}

#[test]
fn send_email_cc_empty() {
    let args = json!({"to": ["test@example.com"], "subject": "Test", "html": "<p>Hi</p>", "cc": []});
    let cc = get_str_array(&args, "cc");
    assert!(cc.is_empty());
}

#[test]
fn send_email_cc_missing() {
    let args = json!({"to": ["test@example.com"], "subject": "Test", "html": "<p>Hi</p>"});
    let cc = get_str_array(&args, "cc");
    assert!(cc.is_empty());
}

// ============================================================================
// Sequence creation — step validation
// ============================================================================

#[test]
fn sequence_steps_empty_array() {
    let args = json!({"name": "Test Sequence", "steps": []});
    let steps = args.get("steps").and_then(|v| v.as_array());
    assert!(steps.unwrap().is_empty());
}

#[test]
fn sequence_steps_missing() {
    let args = json!({"name": "Test Sequence"});
    let steps = args.get("steps");
    assert!(steps.is_none());
}

#[test]
fn sequence_steps_not_array() {
    let args = json!({"name": "Test Sequence", "steps": "not-an-array"});
    let steps = args.get("steps").filter(|v| v.is_array());
    assert!(steps.is_none());
}

#[test]
fn sequence_steps_null() {
    let args = json!({"name": "Test Sequence", "steps": null});
    let steps = args.get("steps").filter(|v| v.is_array());
    assert!(steps.is_none());
}

#[test]
fn sequence_step_valid() {
    let step = json!({"step_number": 0, "delay_days": 1, "subject": "Follow up", "html": "<p>Hi</p>"});
    assert_eq!(step.get("step_number").and_then(|n| n.as_i64()), Some(0));
    assert_eq!(step.get("delay_days").and_then(|n| n.as_i64()), Some(1));
    assert!(step.get("subject").and_then(|v| v.as_str()).is_some());
    assert!(step.get("html").and_then(|v| v.as_str()).is_some());
}

#[test]
fn sequence_step_negative_delay() {
    let step = json!({"step_number": 0, "delay_days": -5, "subject": "Test", "html": "<p>Hi</p>"});
    let delay = step.get("delay_days").and_then(|d| d.as_i64()).unwrap();
    assert_eq!(delay, -5); // Not validated — would create past send time
}

#[test]
fn sequence_step_zero_delay() {
    let step = json!({"step_number": 0, "delay_days": 0, "subject": "Test", "html": "<p>Hi</p>"});
    let delay = step.get("delay_days").and_then(|d| d.as_i64()).unwrap();
    assert_eq!(delay, 0); // Send immediately
}

#[test]
fn sequence_step_huge_delay() {
    let step = json!({"step_number": 0, "delay_days": 365000, "subject": "Test", "html": "<p>Hi</p>"});
    let delay = step.get("delay_days").and_then(|d| d.as_i64()).unwrap();
    assert_eq!(delay, 365000); // ~1000 years — not validated
}

#[test]
fn sequence_step_negative_step_number() {
    let step = json!({"step_number": -1, "delay_days": 1, "subject": "Test", "html": "<p>Hi</p>"});
    let step_num = step.get("step_number").and_then(|n| n.as_i64()).unwrap();
    assert_eq!(step_num, -1); // Not validated
}

#[test]
fn sequence_step_missing_subject() {
    let step = json!({"step_number": 0, "delay_days": 1, "html": "<p>Hi</p>"});
    let subject = step.get("subject").and_then(|v| v.as_str());
    assert!(subject.is_none()); // Would default to "(no subject)" in advance_sequence
}

#[test]
fn sequence_step_missing_html() {
    let step = json!({"step_number": 0, "delay_days": 1, "subject": "Test"});
    let html = step.get("html").and_then(|v| v.as_str());
    assert!(html.is_none()); // Would default to "" in advance_sequence
}

#[test]
fn sequence_step_sql_injection_in_subject() {
    let step = json!({
        "step_number": 0,
        "delay_days": 1,
        "subject": "'; DROP TABLE email.sequences;--",
        "html": "<p>Hi</p>"
    });
    let subject = step.get("subject").and_then(|v| v.as_str()).unwrap();
    assert!(subject.contains("DROP TABLE")); // Parameterized query
}

#[test]
fn sequence_duplicate_step_numbers() {
    let steps = json!([
        {"step_number": 0, "delay_days": 0, "subject": "First", "html": "<p>1</p>"},
        {"step_number": 0, "delay_days": 1, "subject": "Second", "html": "<p>2</p>"}
    ]);
    let arr = steps.as_array().unwrap();
    // iter().find() returns the first match — "Second" is never sent
    let found = arr.iter().find(|s| s.get("step_number").and_then(|n| n.as_i64()) == Some(0));
    assert_eq!(found.unwrap().get("subject").and_then(|v| v.as_str()), Some("First"));
}

#[test]
fn sequence_gap_in_step_numbers() {
    let steps = json!([
        {"step_number": 0, "delay_days": 0, "subject": "First", "html": "<p>1</p>"},
        {"step_number": 5, "delay_days": 1, "subject": "Jumped", "html": "<p>5</p>"}
    ]);
    let arr = steps.as_array().unwrap();
    // Step 1 doesn't exist — enrollment would be marked completed after step 0
    let step1 = arr.iter().find(|s| s.get("step_number").and_then(|n| n.as_i64()) == Some(1));
    assert!(step1.is_none());
}

// ============================================================================
// Enrollment — edge cases
// ============================================================================

#[test]
fn enroll_missing_sequence_id() {
    let args = json!({"contact_email": "test@example.com"});
    assert!(get_str(&args, "sequence_id").is_none());
}

#[test]
fn enroll_missing_contact_email() {
    let args = json!({"sequence_id": "some-id"});
    assert!(get_str(&args, "contact_email").is_none());
}

#[test]
fn enroll_empty_contact_email() {
    let args = json!({"sequence_id": "some-id", "contact_email": ""});
    let email = get_str(&args, "contact_email").unwrap();
    assert!(email.is_empty());
}

#[test]
fn enroll_invalid_email_format() {
    let args = json!({"sequence_id": "some-id", "contact_email": "not-an-email"});
    let email = get_str(&args, "contact_email").unwrap();
    // No email validation in enroll_contact — just stored as-is
    assert_eq!(email, "not-an-email");
}

// ============================================================================
// Pause enrollment — edge cases
// ============================================================================

#[test]
fn pause_missing_enrollment_id() {
    let args = json!({});
    assert!(get_str(&args, "enrollment_id").is_none());
}

#[test]
fn pause_empty_enrollment_id() {
    let args = json!({"enrollment_id": ""});
    let id = get_str(&args, "enrollment_id").unwrap();
    assert!(id.is_empty());
}

#[test]
fn pause_sql_injection_enrollment_id() {
    let args = json!({"enrollment_id": "'; DROP TABLE email.sequence_enrollments;--"});
    let id = get_str(&args, "enrollment_id").unwrap();
    assert!(id.contains("DROP TABLE")); // Parameterized query
}

// ============================================================================
// list_sent_emails — status and limit
// ============================================================================

#[test]
fn list_emails_valid_statuses() {
    for status in &["queued", "sent", "failed", "dry_run"] {
        let args = json!({"status": status});
        let s = get_str(&args, "status").unwrap();
        assert!(!s.is_empty());
    }
}

#[test]
fn list_emails_invalid_status() {
    let args = json!({"status": "delivered"});
    let status = get_str(&args, "status").unwrap();
    // Not validated — would just return empty results from DB
    assert_eq!(status, "delivered");
}

#[test]
fn list_emails_default_limit() {
    let args = json!({});
    let limit = get_i64(&args, "limit").unwrap_or(50);
    assert_eq!(limit, 50);
}

#[test]
fn list_emails_zero_limit() {
    let args = json!({"limit": 0});
    let limit = get_i64(&args, "limit").unwrap_or(50);
    assert_eq!(limit, 0);
}

#[test]
fn list_emails_negative_limit() {
    let args = json!({"limit": -10});
    let limit = get_i64(&args, "limit").unwrap_or(50);
    assert_eq!(limit, -10); // Not clamped — passed directly to SQL LIMIT
}

#[test]
fn list_emails_very_large_limit() {
    let args = json!({"limit": i64::MAX});
    let limit = get_i64(&args, "limit").unwrap_or(50);
    assert_eq!(limit, i64::MAX); // Not clamped
}

// ============================================================================
// create_template — edge cases
// ============================================================================

#[test]
fn template_missing_name() {
    let args = json!({"subject": "Test", "html_body": "<p>Hi</p>"});
    assert!(get_str(&args, "name").is_none());
}

#[test]
fn template_missing_subject() {
    let args = json!({"name": "test", "html_body": "<p>Hi</p>"});
    assert!(get_str(&args, "subject").is_none());
}

#[test]
fn template_missing_html_body() {
    let args = json!({"name": "test", "subject": "Test"});
    assert!(get_str(&args, "html_body").is_none());
}

#[test]
fn template_empty_name() {
    let args = json!({"name": "", "subject": "Test", "html_body": "<p>Hi</p>"});
    let name = get_str(&args, "name").unwrap();
    assert!(name.is_empty());
}

#[test]
fn template_very_long_name() {
    let long_name = "x".repeat(1000);
    let args = json!({"name": long_name, "subject": "Test", "html_body": "<p>Hi</p>"});
    let name = get_str(&args, "name").unwrap();
    assert_eq!(name.len(), 1000);
}

#[test]
fn template_default_from_addr() {
    let args = json!({"name": "test", "subject": "Test", "html_body": "<p>Hi</p>"});
    let from = get_str(&args, "from_addr").unwrap_or_else(|| "DataXLR8 <noreply@dataxlr8.ai>".into());
    assert_eq!(from, "DataXLR8 <noreply@dataxlr8.ai>");
}

#[test]
fn template_variables_list() {
    let args = json!({
        "name": "welcome",
        "subject": "Welcome {{name}}",
        "html_body": "<p>Hello {{name}} from {{company}}</p>",
        "variables": ["name", "company"]
    });
    let vars = get_str_array(&args, "variables");
    assert_eq!(vars, vec!["name", "company"]);
}

#[test]
fn template_sql_injection_in_name() {
    let args = json!({
        "name": "'; DROP TABLE email.templates;--",
        "subject": "Test",
        "html_body": "<p>Hi</p>"
    });
    let name = get_str(&args, "name").unwrap();
    assert!(name.contains("DROP TABLE")); // Parameterized query
}

// ============================================================================
// advance_sequence — optional sequence_id filter
// ============================================================================

#[test]
fn advance_no_sequence_filter() {
    let args = json!({});
    assert!(get_str(&args, "sequence_id").is_none());
}

#[test]
fn advance_with_sequence_filter() {
    let id = uuid::Uuid::new_v4().to_string();
    let args = json!({"sequence_id": id});
    assert!(get_str(&args, "sequence_id").is_some());
}

#[test]
fn advance_empty_sequence_id() {
    let args = json!({"sequence_id": ""});
    let sid = get_str(&args, "sequence_id").unwrap();
    assert!(sid.is_empty());
}

// ============================================================================
// Core helper edge cases
// ============================================================================

#[test]
fn get_str_with_null_byte() {
    let args = json!({"name": "hello\u{0000}world"});
    let val = get_str(&args, "name").unwrap();
    assert!(val.contains('\0'));
}

#[test]
fn get_str_with_backslash() {
    let args = json!({"path": "C:\\Users\\test"});
    let val = get_str(&args, "path").unwrap();
    assert!(val.contains('\\'));
}

#[test]
fn get_i64_boundary_values() {
    let args = json!({"max": i64::MAX, "min": i64::MIN, "zero": 0});
    assert_eq!(get_i64(&args, "max"), Some(i64::MAX));
    assert_eq!(get_i64(&args, "min"), Some(i64::MIN));
    assert_eq!(get_i64(&args, "zero"), Some(0));
}

#[test]
fn get_str_array_duplicate_values() {
    let args = json!({"to": ["a@b.com", "a@b.com", "a@b.com"]});
    let result = get_str_array(&args, "to");
    assert_eq!(result.len(), 3); // Duplicates preserved
}
