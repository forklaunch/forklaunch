use super::continuation::{
    CONTINUATION_TTL_MS, Continuation, ContinuationError, MAX_CONTINUATION_BYTES, PendingToolCall,
    now_ms, open_continuation, seal_continuation,
};
use super::*;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const SECRET: &str = "test-hmac-secret";

/// Scripted model: each call pops the next response and records the request.
#[derive(Default)]
struct FakeModel {
    responses: Mutex<VecDeque<Result<Value, ModelError>>>,
    chunks: Mutex<VecDeque<Vec<Value>>>,
    requests: Mutex<Vec<Value>>,
}
impl FakeModel {
    fn new(responses: Vec<Value>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into_iter().map(Ok).collect()),
            ..Self::default()
        })
    }
    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
}
struct Chunks(VecDeque<Value>);
impl ModelChunks for Chunks {
    fn next(&mut self) -> BoxFuture<'_, Result<Option<Value>, ModelError>> {
        let next = self.0.pop_front();
        Box::pin(async move { Ok(next) })
    }
}
impl ModelBackend for FakeModel {
    fn complete(&self, request: Value) -> BoxFuture<'_, Result<Value, ModelError>> {
        self.requests.lock().unwrap().push(request);
        let next = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(answer("out of script")));
        Box::pin(async move { next })
    }
    fn stream(&self, request: Value) -> BoxFuture<'_, Result<Box<dyn ModelChunks>, ModelError>> {
        self.requests.lock().unwrap().push(request);
        let chunks = self.chunks.lock().unwrap().pop_front().unwrap_or_default();
        Box::pin(async move { Ok(Box::new(Chunks(chunks.into())) as Box<dyn ModelChunks>) })
    }
}

fn answer(text: &str) -> Value {
    json!({"choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}]})
}
fn tool_call(id: &str, name: &str, arguments: &str) -> Value {
    json!({"choices": [{"index": 0, "message": {"role": "assistant", "content": null, "tool_calls": [
        {"id": id, "type": "function", "function": {"name": name, "arguments": arguments}}
    ]}, "finish_reason": "tool_calls"}]})
}

fn counter_tool(name: &str, calls: Arc<AtomicUsize>) -> Tool {
    Tool::new(name, "", format!("{name} tool"), json!({"type": "object"})).handler(
        move |input, context| {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(json!({"input": input, "user": context.user_id}))
            }
        },
    )
}

fn agent(model: &Arc<FakeModel>, tools: Vec<Tool>) -> Agent {
    let mut builder = Agent::builder("assistant")
        .instructions("Be useful.")
        .model("luna")
        .continuation_secret(SECRET)
        .streaming(false)
        .model_backend(model.clone());
    for tool in tools {
        builder = builder.tool(tool);
    }
    builder.build().unwrap()
}

fn caller() -> AgentCaller {
    AgentCaller::new("user-1").with_authorization(Some("Bearer user-token".into()))
}

fn start(task: &str) -> AgentTaskRequest {
    AgentTaskRequest::Start { task: task.into() }
}

#[test]
fn tool_names_follow_the_mcp_pattern() {
    for name in ["a", "lookup_order", "a1_b2", &"a".repeat(64)] {
        assert!(valid_tool_name(name), "{name}");
    }
    for name in [
        "",
        "1abc",
        "_a",
        "Lookup",
        "a-b",
        "a.b",
        &"a".repeat(65),
        "é",
    ] {
        assert!(!valid_tool_name(name), "{name}");
    }
    assert_eq!(tool_name_segment("GetRecord"), "get_record");
    assert_eq!(tool_name_segment("list-Notes v2"), "list_notes_v2");
    assert_eq!(
        prefixed_tool_name("case-notes", "listNotes"),
        "case_notes__list_notes"
    );
    assert_eq!(
        prefixed_tool_name("svc", &"x".repeat(80)).len(),
        MAX_TOOL_NAME
    );
    let model = FakeModel::new(vec![]);
    let bad = Tool::new("Bad-Name", "", "d", json!({}));
    let error = Agent::builder("a")
        .model_backend(model.clone())
        .tool(bad)
        .build()
        .unwrap_err();
    assert!(error.message.contains("lowercase snake_case"));
    let empty = Tool::new("ok", "", " ", json!({}));
    assert!(
        Agent::builder("a")
            .model_backend(model)
            .tool(empty)
            .build()
            .is_err()
    );
}

struct FixedSource(&'static str, Vec<Tool>);
impl ToolSource for FixedSource {
    fn kind(&self) -> ToolSourceKind {
        ToolSourceKind::Service
    }
    fn name(&self) -> &str {
        self.0
    }
    fn list<'a>(&'a self, _: &'a ToolContext, _: bool) -> BoxFuture<'a, Result<Vec<Tool>, String>> {
        let tools = self.1.clone();
        Box::pin(async move { Ok(tools) })
    }
}

#[tokio::test]
async fn name_collisions_fail() {
    let model = FakeModel::new(vec![]);
    let calls = Arc::new(AtomicUsize::new(0));
    let duplicate = Agent::builder("a")
        .model_backend(model.clone())
        .tool(counter_tool("same", calls.clone()))
        .tool(counter_tool("same", calls.clone()))
        .build()
        .unwrap_err();
    assert!(
        duplicate
            .message
            .contains("Two agent tools are named \"same\"")
    );
    let across = Agent::builder("a")
        .model("luna")
        .model_backend(model.clone())
        .tool(counter_tool("records__get", calls.clone()))
        .source(FixedSource(
            "records",
            vec![counter_tool("records__get", calls.clone())],
        ))
        .build()
        .unwrap();
    let error = across.check().await.unwrap_err();
    assert_eq!(error.status, 503);
    assert!(
        error.message.contains("(a and records)"),
        "{}",
        error.message
    );
    assert!(across.list_tools(&caller()).await.is_err());
    let sources = Agent::builder("a")
        .model_backend(model)
        .source(FixedSource("x", vec![]))
        .source(FixedSource("x", vec![]))
        .build()
        .unwrap_err();
    assert!(sources.message.contains("sources are named \"x\""));
}

#[tokio::test]
async fn annotations_decide_approval_in_the_listing() {
    let model = FakeModel::new(vec![]);
    let calls = Arc::new(AtomicUsize::new(0));
    let agent = agent(
        &model,
        vec![
            counter_tool("read", calls.clone()).read_only(),
            counter_tool("write", calls.clone()),
            counter_tool("wipe", calls.clone()).destructive(),
            counter_tool("put", calls.clone()).annotations(ToolAnnotations {
                read_only_hint: Some(false),
                idempotent_hint: Some(true),
                ..ToolAnnotations::default()
            }),
        ],
    );
    let listing = serde_json::to_value(agent.list_tools(&caller()).await.unwrap()).unwrap();
    assert_eq!(listing["agent"], "assistant");
    assert_eq!(listing["model"], "luna");
    let by_name = |name: &str| {
        listing["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == name)
            .cloned()
            .unwrap()
    };
    assert_eq!(
        by_name("read"),
        json!({
            "name": "read", "description": "read tool", "inputSchema": {"type": "object"},
            "annotations": {"readOnlyHint": true}, "source": {"kind": "local", "name": "assistant"},
            "requiresApproval": false, "available": true, "unavailableReason": null
        })
    );
    assert_eq!(by_name("write")["requiresApproval"], true);
    assert_eq!(by_name("write")["annotations"], json!({}));
    assert_eq!(
        by_name("wipe")["annotations"],
        json!({"readOnlyHint": false, "destructiveHint": true})
    );
    assert_eq!(by_name("put")["requiresApproval"], true);
    assert_eq!(method_annotations("GET").read_only_hint, Some(true));
    assert_eq!(method_annotations("head").read_only_hint, Some(true));
    assert_eq!(method_annotations("delete").destructive_hint, Some(true));
    assert_eq!(method_annotations("put").idempotent_hint, Some(true));
    assert!(method_annotations("post").requires_approval());
}

fn sample(user: &str, expires_at: i64) -> Continuation {
    Continuation {
        v: 1,
        user_id: user.into(),
        expires_at,
        steps: 1,
        messages: vec![json!({"role": "user", "content": "hi"})],
        pending: vec![PendingToolCall {
            id: "c1".into(),
            name: "write".into(),
            arguments: "{}".into(),
        }],
        tool_calls: vec![],
    }
}

#[test]
fn continuations_are_sealed_bound_and_expire() {
    let now = now_ms();
    let value = sample("user-1", now + CONTINUATION_TTL_MS);
    let token = seal_continuation(&value, SECRET).unwrap();
    assert!(!token.contains("write"), "tool calls stay confidential");
    assert_eq!(
        open_continuation(&token, SECRET, "user-1", now),
        Some(value.clone())
    );
    // Tampering anywhere in the token is detected.
    let mut bytes = token.clone().into_bytes();
    let middle = bytes.len() / 2;
    bytes[middle] = if bytes[middle] == b'A' { b'B' } else { b'A' };
    let tampered = String::from_utf8(bytes).unwrap();
    assert_eq!(open_continuation(&tampered, SECRET, "user-1", now), None);
    assert_eq!(
        open_continuation(&token, "other-secret", "user-1", now),
        None
    );
    assert_eq!(open_continuation(&token, SECRET, "user-2", now), None);
    assert_eq!(
        open_continuation(&token, SECRET, "user-1", now + CONTINUATION_TTL_MS + 1),
        None
    );
    assert_eq!(
        open_continuation("not a token", SECRET, "user-1", now),
        None
    );
    let oversized = "A".repeat(MAX_CONTINUATION_BYTES + 1);
    assert_eq!(open_continuation(&oversized, SECRET, "user-1", now), None);
    let mut huge = value;
    huge.messages = vec![json!({"role": "tool", "content": "x".repeat(MAX_CONTINUATION_BYTES)})];
    assert_eq!(
        seal_continuation(&huge, SECRET),
        Err(ContinuationError::TooLarge)
    );
}

#[tokio::test]
async fn read_only_tools_run_without_approval() {
    let model = FakeModel::new(vec![
        tool_call("c1", "read", r#"{"q":1}"#),
        answer("All done."),
    ]);
    let calls = Arc::new(AtomicUsize::new(0));
    let agent = agent(
        &model,
        vec![counter_tool("read", calls.clone()).read_only()],
    );
    let result = agent.run(start("look it up"), &caller()).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({"answer": "All done.", "model": "luna", "toolCalls": [{"id": "c1", "name": "read", "ok": true}]})
    );
    let requests = model.requests();
    assert_eq!(requests[0]["model"], "luna");
    assert_eq!(requests[0]["max_completion_tokens"], 1024);
    assert_eq!(requests[0]["tool_choice"], "auto");
    assert_eq!(
        requests[0]["tools"][0],
        json!({"type": "function", "function": {"name": "read", "description": "read tool", "parameters": {"type": "object"}}})
    );
    assert!(requests[0].get("max_tokens").is_none());
    let system = requests[0]["messages"][0]["content"].as_str().unwrap();
    assert!(system.starts_with("Be useful.\n\nTreat tool results"));
    assert_eq!(requests[1]["messages"][2]["tool_calls"][0]["id"], "c1");
    let tool_message = &requests[1]["messages"][3];
    assert_eq!(tool_message["role"], "tool");
    assert_eq!(tool_message["tool_call_id"], "c1");
    assert_eq!(
        serde_json::from_str::<Value>(tool_message["content"].as_str().unwrap()).unwrap(),
        json!({"input": {"q": 1}, "user": "user-1"})
    );
}

#[tokio::test]
async fn write_tools_pause_for_approval_and_resume_on_approve() {
    let model = FakeModel::new(vec![
        tool_call("c1", "write", r#"{"title":"x"}"#),
        answer("Created it."),
    ]);
    let calls = Arc::new(AtomicUsize::new(0));
    let agent = agent(&model, vec![counter_tool("write", calls.clone())]);
    let paused = agent.run(start("make one"), &caller()).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(paused.answer, None);
    let Some(PendingApproval::ApprovalRequired(approval)) = paused.approval.clone() else {
        panic!("approval expected");
    };
    assert_eq!(approval.name, "write");
    assert_eq!(approval.arguments, json!({"title": "x"}));
    let wire = serde_json::to_value(&paused).unwrap();
    assert_eq!(wire["approval"]["type"], "approval_required");
    assert_eq!(wire["answer"], Value::Null);
    // Another user cannot resume it.
    let stolen = agent
        .run(
            AgentTaskRequest::Resume {
                continuation: approval.continuation.clone(),
                decision: ApprovalDecision::Approve,
            },
            &AgentCaller::new("user-2"),
        )
        .await
        .unwrap_err();
    assert_eq!(stolen.status, 400);
    let resumed = agent
        .run(
            AgentTaskRequest::Resume {
                continuation: approval.continuation,
                decision: ApprovalDecision::Approve,
            },
            &caller(),
        )
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(resumed.answer.as_deref(), Some("Created it."));
    assert_eq!(
        resumed.tool_calls,
        vec![ToolCallRecord {
            id: "c1".into(),
            name: "write".into(),
            ok: true
        }]
    );
}

#[tokio::test]
async fn deny_tells_the_model_the_user_declined() {
    let model = FakeModel::new(vec![
        tool_call("c1", "write", "{}"),
        answer("Okay, I did not change anything."),
    ]);
    let calls = Arc::new(AtomicUsize::new(0));
    let agent = agent(&model, vec![counter_tool("write", calls.clone())]);
    let paused = agent.run(start("make one"), &caller()).await.unwrap();
    let Some(PendingApproval::ApprovalRequired(approval)) = paused.approval else {
        panic!("approval expected");
    };
    let result = agent
        .run(
            AgentTaskRequest::Resume {
                continuation: approval.continuation,
                decision: ApprovalDecision::Deny,
            },
            &caller(),
        )
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!result.tool_calls[0].ok);
    let last = model.requests().pop().unwrap();
    assert_eq!(
        last["messages"][3],
        json!({"role": "tool", "tool_call_id": "c1", "content": "The user declined this action. It did not run."})
    );
}

#[tokio::test]
async fn step_cap_forces_a_final_answer_without_tools() {
    let model = FakeModel::new(vec![
        tool_call("c1", "read", "{}"),
        tool_call("c2", "read", "{}"),
        answer("Final."),
    ]);
    let calls = Arc::new(AtomicUsize::new(0));
    let agent = Agent::builder("assistant")
        .model("luna")
        .streaming(false)
        .limits(AgentLimits {
            max_steps: 2,
            ..AgentLimits::default()
        })
        .model_backend(model.clone())
        .tool(counter_tool("read", calls.clone()).read_only())
        .build()
        .unwrap();
    let result = agent.run(start("loop"), &caller()).await.unwrap();
    assert_eq!(result.answer.as_deref(), Some("Final."));
    let requests = model.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[1].get("tools").is_some());
    assert!(requests[2].get("tools").is_none());
    assert!(requests[2].get("tool_choice").is_none());
}

#[tokio::test]
async fn long_tool_results_are_truncated_with_a_marker() {
    let model = FakeModel::new(vec![tool_call("c1", "big", "{}"), answer("ok")]);
    let big = Tool::new("big", "", "big", json!({"type": "object"}))
        .read_only()
        .handler(|_, _| async { Ok(Value::String("é".repeat(40_000))) });
    let agent = agent(&model, vec![big]);
    agent.run(start("go"), &caller()).await.unwrap();
    let content = model.requests()[1]["messages"][3]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(content.ends_with("\n[truncated]"));
    assert!(content.len() <= 64 * 1024 + "\n[truncated]".len());
}

#[tokio::test]
async fn task_length_is_bounded() {
    let model = FakeModel::new(vec![]);
    let agent = agent(&model, vec![]);
    let error = agent
        .run(start(&"x".repeat(8001)), &caller())
        .await
        .unwrap_err();
    assert_eq!(error.status, 400);
    assert_eq!(error.message, "Enter a task of up to 8,000 characters.");
    assert!(agent.run(start("   "), &caller()).await.is_err());
    assert!(agent.run(start(&"x".repeat(8000)), &caller()).await.is_ok());
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct Input {
    n: u32,
}

#[tokio::test]
async fn tool_failures_and_timeouts_are_reported_to_the_model() {
    let model = FakeModel::new(vec![
        json!({"choices": [{"index": 0, "message": {"role": "assistant", "content": null, "tool_calls": [
            {"id": "c1", "type": "function", "function": {"name": "slow", "arguments": "{}"}},
            {"id": "c2", "type": "function", "function": {"name": "fails", "arguments": "{}"}},
            {"id": "c3", "type": "function", "function": {"name": "typed", "arguments": "{\"n\":\"x\"}"}},
            {"id": "c4", "type": "function", "function": {"name": "missing", "arguments": "{}"}},
            {"id": "c5", "type": "function", "function": {"name": "typed", "arguments": "{oops"}}
        ]}, "finish_reason": "tool_calls"}]}),
        answer("Reported."),
    ]);
    let agent = Agent::builder("assistant")
        .model("luna")
        .streaming(false)
        .limits(AgentLimits {
            tool_timeout: std::time::Duration::from_millis(50),
            ..AgentLimits::default()
        })
        .model_backend(model.clone())
        .tool(
            Tool::new("slow", "", "slow", json!({}))
                .read_only()
                .handler(|_, _| async {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    Ok(Value::Null)
                }),
        )
        .tool(
            Tool::new("fails", "", "fails", json!({}))
                .read_only()
                .handler(|_, _| async { Err(ToolError::new("x".repeat(500))) }),
        )
        .tool(
            Tool::with_input::<Input>("typed", "Typed", "typed")
                .read_only()
                .typed_handler(|input: Input, _| async move { Ok(json!(input.n)) }),
        )
        .build()
        .unwrap();
    let mut events = Vec::new();
    agent
        .stream(start("go"), &caller(), &mut |event| events.push(event))
        .await;
    let results: Vec<(String, bool, String)> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolResult { id, ok, summary } => Some((id.clone(), *ok, summary.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(results[0].2, "The tool did not finish within 50 ms.");
    assert_eq!(results[1].2.chars().count(), 200);
    assert!(results[1].2.ends_with("..."));
    assert_eq!(results[2].2, "Invalid arguments.");
    assert_eq!(results[3].2, "Unknown tool.");
    assert_eq!(results[4].2, "Invalid arguments.");
    assert!(results.iter().all(|(_, ok, _)| !ok));
    let schema = input_schema::<Input>();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["n"].is_object());
}

fn snapshot(events: &[AgentEvent]) -> Vec<Value> {
    events
        .iter()
        .map(|event| {
            let mut value = serde_json::to_value(event).unwrap();
            if value.get("continuation").is_some() {
                value["continuation"] = json!("<token>");
            }
            value
        })
        .collect()
}

#[tokio::test]
async fn stream_events_have_the_contract_shape() {
    let model = FakeModel::new(vec![
        tool_call("c1", "read", r#"{"id":7}"#),
        tool_call("c2", "write", r#"{"title":"t"}"#),
        answer("Done."),
    ]);
    let calls = Arc::new(AtomicUsize::new(0));
    let agent = agent(
        &model,
        vec![
            counter_tool("read", calls.clone()).read_only(),
            counter_tool("write", calls.clone()).destructive(),
        ],
    );
    let mut receiver = agent.spawn_stream(start("do it"), caller());
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    assert_eq!(
        snapshot(&events),
        vec![
            json!({"type": "status", "message": "Agent is working"}),
            json!({"type": "tool_call", "id": "c1", "name": "read", "arguments": {"id": 7}}),
            json!({"type": "tool_result", "id": "c1", "ok": true, "summary": "Completed."}),
            json!({"type": "tool_call", "id": "c2", "name": "write", "arguments": {"title": "t"}}),
            json!({"type": "approval_required", "id": "c2", "name": "write", "arguments": {"title": "t"},
                   "annotations": {"readOnlyHint": false, "destructiveHint": true}, "continuation": "<token>"}),
        ]
    );
    let AgentEvent::ApprovalRequired(approval) = events.pop().unwrap() else {
        panic!("approval expected");
    };
    let mut receiver = agent.spawn_stream(
        AgentTaskRequest::Resume {
            continuation: approval.continuation,
            decision: ApprovalDecision::Approve,
        },
        caller(),
    );
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    assert_eq!(
        snapshot(&events),
        vec![
            json!({"type": "status", "message": "Running the approved action"}),
            json!({"type": "tool_result", "id": "c2", "ok": true, "summary": "Completed."}),
            json!({"type": "delta", "text": "Done."}),
            json!({"type": "done", "model": "luna"}),
        ]
    );
}

#[tokio::test]
async fn streamed_tool_calls_are_joined_by_index() {
    let model = Arc::new(FakeModel::default());
    model.chunks.lock().unwrap().extend([
        vec![
            json!({"choices": [{"index": 0, "delta": {"content": "Let me check. "}}]}),
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": "s1", "function": {"name": "re", "arguments": "{\"a\""}}]}}]}),
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"name": "ad", "arguments": ":1}"}}]}}]}),
            json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
        ],
        vec![
            json!({"choices": [{"index": 0, "delta": {"content": "Hel"}}]}),
            json!({"choices": [{"index": 0, "delta": {"content": "lo"}, "finish_reason": "stop"}]}),
        ],
        vec![json!({"choices": [{"index": 0, "delta": {"content": "cut"}}]})],
    ]);
    let calls = Arc::new(AtomicUsize::new(0));
    let agent = Agent::builder("assistant")
        .model("luna")
        .streaming(true)
        .model_backend(model.clone())
        .tool(counter_tool("read", calls.clone()).read_only())
        .build()
        .unwrap();
    let result = agent.run(start("hi"), &caller()).await.unwrap();
    assert_eq!(result.answer.as_deref(), Some("Hello"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let requests = model.requests();
    assert_eq!(
        requests[1]["messages"][2]["tool_calls"][0]["function"],
        json!({"name": "read", "arguments": "{\"a\":1}"})
    );
    // A stream that ends without finish_reason is not an answer.
    let error = agent.run(start("again"), &caller()).await.unwrap_err();
    assert_eq!(error.status, 502);
}

#[tokio::test]
async fn model_failures_become_error_events() {
    let model = Arc::new(FakeModel {
        responses: Mutex::new(VecDeque::from([Err(ModelError::status(429))])),
        ..FakeModel::default()
    });
    let agent = agent(&model, vec![]);
    let mut events = Vec::new();
    agent
        .stream(start("hi"), &caller(), &mut |event| events.push(event))
        .await;
    assert_eq!(
        snapshot(&events),
        vec![
            json!({"type": "status", "message": "Agent is working"}),
            json!({"type": "error", "message": "Model allowance reached. Review the model connection."}),
        ]
    );
    let unconfigured = Agent::builder("a").model_backend(model).build().unwrap();
    // No AGENT_MODEL in the test environment.
    if std::env::var("AGENT_MODEL").is_err() {
        let error = unconfigured.run(start("hi"), &caller()).await.unwrap_err();
        assert_eq!(error.status, 503);
        assert_eq!(
            unconfigured.list_tools(&caller()).await.unwrap().model,
            None
        );
    }
}

#[tokio::test]
async fn streams_stop_at_the_byte_limit() {
    let model = Arc::new(FakeModel::default());
    model.chunks.lock().unwrap().push_back(
        (0..100)
            .map(|_| json!({"choices": [{"index": 0, "delta": {"content": "x".repeat(100)}}]}))
            .collect(),
    );
    let agent = Agent::builder("assistant")
        .model("luna")
        .streaming(true)
        .limits(AgentLimits {
            max_stream_bytes: 2048,
            ..AgentLimits::default()
        })
        .model_backend(model)
        .build()
        .unwrap();
    let mut receiver = agent.spawn_stream(start("hi"), caller());
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    let total: usize = events
        .iter()
        .map(|event| serde_json::to_vec(event).unwrap().len())
        .sum();
    assert!(total <= 2048, "{total}");
    assert!(matches!(events.last(), Some(AgentEvent::Error { .. })));
}

#[tokio::test]
async fn unavailable_sources_are_listed_and_not_offered() {
    let model = FakeModel::new(vec![answer("fine")]);
    let calls = Arc::new(AtomicUsize::new(0));
    let agent = Agent::builder("assistant")
        .model("luna")
        .streaming(false)
        .model_backend(model.clone())
        .tool(counter_tool("read", calls).read_only())
        .service(service_tools("zz-missing-svc"))
        .service(service_tools("down").url("http://127.0.0.1:9"))
        .mcp_server(McpServer::new("linear", None))
        .build()
        .unwrap();
    let listing = serde_json::to_value(agent.list_tools(&caller()).await.unwrap()).unwrap();
    let tools = listing["tools"].as_array().unwrap();
    let unavailable: Vec<_> = tools.iter().filter(|t| t["available"] == false).collect();
    assert_eq!(unavailable.len(), 3);
    assert_eq!(
        *unavailable[0],
        json!({
            "name": "zz-missing-svc__unavailable", "description": "Tools from zz-missing-svc could not be loaded.",
            "inputSchema": {"type": "object"}, "annotations": {}, "source": {"kind": "service", "name": "zz-missing-svc"},
            "requiresApproval": true, "available": false,
            "unavailableReason": "Set ZZ_MISSING_SVC_URL to reach the zz-missing-svc service."
        })
    );
    assert_eq!(
        unavailable[1]["unavailableReason"],
        "The down service did not return its API description."
    );
    assert_eq!(
        unavailable[2]["unavailableReason"],
        "Set the URL setting for the linear MCP server."
    );
    assert_eq!(unavailable[2]["source"]["kind"], "mcp");
    agent.run(start("hi"), &caller()).await.unwrap();
    let offered: Vec<_> = model.requests()[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["function"]["name"].clone())
        .collect();
    assert_eq!(offered, vec![json!("read")]);
}

#[test]
fn service_paths_and_settings() {
    assert_eq!(service_url_setting("records"), "RECORDS_URL");
    assert_eq!(service_url_setting("case-notes"), "CASE_NOTES_URL");
    for path in [
        "/health",
        "/api/v1/openapi",
        "/docs/x",
        "/mcp",
        "/api/v1/agent/tools",
        "/ready",
    ] {
        assert!(skipped_path(path), "{path}");
    }
    for path in ["/records/{id}", "/healthz", "/openapi.json"] {
        assert!(!skipped_path(path), "{path}");
    }
}

/// An app service serving its `OpenAPI` document and two routes.
async fn records_service() -> (String, Arc<Mutex<Vec<Option<String>>>>) {
    use axum::{Json, Router, extract::Path, http::HeaderMap, routing::get};
    let seen = Arc::new(Mutex::new(Vec::new()));
    let document = json!({
        "openapi": "3.1.0",
        "paths": {
            "/records/{id}": {
                "get": {"operationId": "getRecord", "summary": "Get record",
                        "responses": {"200": {"content": {"application/json": {"schema": {"type": "object"}}}}, "404": {"content": {"text/plain": {}}}},
                        "parameters": [{"in": "path", "name": "id", "required": true, "schema": {"$ref": "#/components/schemas/Id"}},
                                       {"in": "query", "name": "fields", "schema": {"type": "string"}}]},
                "delete": {"operationId": "deleteRecord", "summary": "Delete record",
                           "parameters": [{"in": "path", "name": "id", "required": true, "schema": {"type": "string"}}]}
            },
            "/records": {
                "post": {"operationId": "createRecord", "requestBody": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/Record"}}}}},
                "put": {"operationId": "uploadRecord", "requestBody": {"content": {"multipart/form-data": {"schema": {}}}}}
            },
            "/health": {"get": {"operationId": "health"}},
            "/internal/records/{id}/progress": {"post": {"operationId": "recordProgress", "x-forklaunch-access": "internal",
                "requestBody": {"content": {"application/json": {"schema": {"type": "object"}}}}}},
            "/records/{id}/sync": {"post": {"operationId": "syncRecord", "security": [{"hmacAuth": []}]}},
            "/records/{id}/events": {"get": {"operationId": "watchRecord",
                "responses": {"200": {"content": {"text/event-stream": {"schema": {"type": "string"}}}}}}},
            "/records/{id}/file": {"get": {"operationId": "downloadRecord",
                "responses": {"200": {"content": {"application/octet-stream": {"schema": {}}}}}}},
            "/records/{id}/archive": {"post": {"operationId": "archiveRecord", "x-mcp-annotations": {"readOnlyHint": false, "idempotentHint": true},
                "parameters": [{"in": "path", "name": "id", "required": true, "schema": {"type": "string"}}]}}
        },
        "components": {"securitySchemes": {"hmacAuth": {"type": "http", "scheme": "hmac"}}, "schemas": {
            "Id": {"type": "string"},
            "Record": {"type": "object", "properties": {"title": {"type": "string"}, "parent": {"$ref": "#/components/schemas/Record"}}}
        }}
    });
    let recorded = seen.clone();
    let app = Router::new()
        .route(
            "/api/v1/openapi",
            get(move || {
                let document = document.clone();
                async move { Json(json!({"latest": document, "v1": document})) }
            }),
        )
        .route(
            "/records/{id}",
            get(
                move |Path(id): Path<String>,
                      axum::extract::RawQuery(query): axum::extract::RawQuery,
                      headers: HeaderMap| {
                    let recorded = recorded.clone();
                    async move {
                        recorded.lock().unwrap().push(
                            headers
                                .get("authorization")
                                .map(|value| value.to_str().unwrap().to_owned()),
                        );
                        Json(json!({"id": id, "query": query}))
                    }
                },
            ),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, seen)
}

#[tokio::test]
async fn service_tools_come_from_openapi_and_forward_authorization() {
    let (base, seen) = records_service().await;
    let model = FakeModel::new(vec![
        tool_call(
            "c1",
            "records__get_record",
            r#"{"params":{"id":"a b/c"},"query":{"fields":"title"}}"#,
        ),
        answer("Found it."),
    ]);
    let agent = Agent::builder("assistant")
        .model("luna")
        .streaming(false)
        .continuation_secret(SECRET)
        .model_backend(model.clone())
        .service(service_tools("records").url(format!("{base}/")))
        .build()
        .unwrap();
    let listing = agent.list_tools(&caller()).await.unwrap();
    let names: Vec<_> = listing
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "records__archive_record",
            "records__create_record",
            "records__delete_record",
            "records__get_record"
        ]
    );
    let get = &listing.tools[3];
    assert_eq!(get.title.as_deref(), Some("Get record"));
    assert!(!get.requires_approval);
    assert_eq!(
        get.input_schema,
        json!({"type": "object", "required": ["params"], "properties": {
            "params": {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]},
            "query": {"type": "object", "properties": {"fields": {"type": "string"}}, "required": []}
        }})
    );
    let create = &listing.tools[1];
    assert_eq!(create.input_schema["required"], json!(["body"]));
    assert_eq!(
        create.input_schema["properties"]["body"]["properties"]["parent"],
        json!({"type": "object"})
    );
    assert!(create.requires_approval);
    assert_eq!(listing.tools[2].annotations.destructive_hint, Some(true));
    assert_eq!(listing.tools[0].annotations.idempotent_hint, Some(true));
    assert_eq!(listing.tools[0].source.kind, ToolSourceKind::Service);
    let result = agent.run(start("find a b/c"), &caller()).await.unwrap();
    assert!(result.tool_calls[0].ok);
    assert_eq!(
        *seen.lock().unwrap(),
        vec![Some("Bearer user-token".to_owned())]
    );
    let content = model.requests()[1]["messages"][3]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        serde_json::from_str::<Value>(&content).unwrap(),
        json!({"id": "a b/c", "query": "fields=title"})
    );
}

#[tokio::test]
async fn service_tools_offer_read_only_and_allowlisted_operations() {
    let (base, _) = records_service().await;
    let names = |tools: ServiceTools| async move {
        let agent = Agent::builder("assistant")
            .model("luna")
            .streaming(false)
            .continuation_secret(SECRET)
            .model_backend(FakeModel::new(vec![]))
            .service(tools)
            .build()
            .unwrap();
        agent
            .list_tools(&caller())
            .await
            .unwrap()
            .tools
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(service_tools("records").url(&base).read_only()).await,
        vec!["records__get_record"]
    );
    // An allowlist never re-admits internal, HMAC or streaming operations.
    assert_eq!(
        names(service_tools("records").url(&base).include([
            "createRecord",
            "recordProgress",
            "syncRecord",
            "watchRecord"
        ]))
        .await,
        vec!["records__create_record"]
    );
}
