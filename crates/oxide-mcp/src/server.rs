//! Read-only MCP session over newline-delimited JSON-RPC.
use oxide_core::Project;
use serde_json::{json, Value};
use std::io::{self, BufRead, Read, Write};

const VERSION: &str = "2025-06-18";
const SUMMARY: &str = "get_project_summary";
const MAX_LINE: usize = 64 * 1024;

#[derive(PartialEq)]
enum Phase {
    New,
    Initializing,
    Ready,
}

/// Serve the validated project snapshot until the client closes its input.
pub fn serve(project: &Project, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut phase = Phase::New;
    let mut line = Vec::new();
    loop {
        line.clear();
        let size = input
            .by_ref()
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if size == 0 {
            return Ok(());
        }
        let response = if size > MAX_LINE {
            if !line.ends_with(b"\n") {
                input.skip_until(b'\n')?;
            }
            Some(error(&Value::Null, -32600, "Message exceeds 64 KiB"))
        } else {
            match serde_json::from_slice(&line) {
                Ok(request) => dispatch(&request, &mut phase, project),
                Err(_) => Some(error(&Value::Null, -32700, "Invalid JSON")),
            }
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
}

fn error(id: &Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn dispatch(request: &Value, phase: &mut Phase, project: &Project) -> Option<Value> {
    let method = request["method"].as_str();
    let id = request.get("id");
    if request["jsonrpc"] != "2.0"
        || method.is_none()
        || id.is_some_and(|v| !(v.is_string() || v.is_i64() || v.is_u64()))
    {
        return Some(error(&Value::Null, -32600, "Invalid JSON-RPC request"));
    }
    let params = &request["params"];
    if request.get("params").is_some() && !params.is_object() {
        return id.map(|id| error(id, -32602, "Expected a params object"));
    }
    let Some(id) = id else {
        if method == Some("notifications/initialized") && *phase == Phase::Initializing {
            *phase = Phase::Ready;
        }
        return None;
    };
    let result = match method {
        Some("ping") => json!({}),
        Some("initialize") if *phase == Phase::New => {
            if !params["protocolVersion"].is_string()
                || !params["capabilities"].is_object()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string()
            {
                return Some(error(
                    id,
                    -32602,
                    "Expected protocolVersion, capabilities and clientInfo",
                ));
            }
            *phase = Phase::Initializing;
            json!({"protocolVersion":VERSION,"capabilities":{"tools":{}},
                "serverInfo":{"name":"OxideDAW","version":env!("CARGO_PKG_VERSION")}})
        }
        Some("initialize") => return Some(error(id, -32600, "Session already initialized")),
        _ if *phase != Phase::Ready => {
            return Some(error(id, -32002, "Session is not initialized"))
        }
        Some("tools/list") => {
            if params.get("cursor").is_some() {
                return Some(error(id, -32602, "Unknown tool list cursor"));
            }
            json!({"tools":[{"name":SUMMARY,"description":"Read the saved project snapshot loaded at startup.",
                "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
                "annotations":{"readOnlyHint":true}}]})
        }
        Some("tools/call") => {
            if params["name"] != SUMMARY {
                return Some(error(id, -32602, "Expected tool name get_project_summary"));
            }
            if params
                .get("arguments")
                .is_some_and(|v| !v.as_object().is_some_and(|args| args.is_empty()))
            {
                return Some(error(
                    id,
                    -32602,
                    "get_project_summary takes an empty arguments object",
                ));
            }
            let summary = summary(project);
            json!({"content":[{"type":"text","text":summary.to_string()}],"structuredContent":summary})
        }
        _ => return Some(error(id, -32601, "Unknown method")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

fn summary(project: &Project) -> Value {
    // Legacy documents permit long names. Bound display text independently of file size.
    let display_name = |name: &str| name.chars().take(256).collect::<String>();
    let tracks: Vec<_> = project.tracks.iter().map(|track| json!({
        "id":track.id,"name":display_name(&track.name),"sampleAssigned":track.sample.is_some()
    })).collect();
    json!({"name":display_name(&project.name),"formatVersion":project.format_version,
        "bpm":project.bpm,"sampleRate":project.sample_rate,"masterVolume":project.master_volume,
        "tracks":tracks,"namedPatternCount":project.patterns.len(),"arrangementBars":project.arrangement.len()})
}
