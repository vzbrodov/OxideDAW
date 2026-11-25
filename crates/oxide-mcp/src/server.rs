//! Project MCP session over newline-delimited JSON-RPC.
use oxide_core::{
    edit::{EditCommand, ProjectEditor},
    Project,
};
use serde_json::{json, Value};
use std::io::{self, BufRead, Read, Write};
use std::path::Path;

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
    let editor =
        ProjectEditor::new(project.clone()).map_err(|err| io::Error::other(err.to_string()))?;
    serve_session(editor, None, &mut input, &mut output)
}

/// Run a session with explicit editing and saving to the launch document.
pub fn serve_editable(
    editor: ProjectEditor,
    path: &Path,
    input: impl BufRead,
    output: impl Write,
) -> io::Result<()> {
    serve_session(editor, Some(path), input, output)
}

fn serve_session(
    mut editor: ProjectEditor,
    path: Option<&Path>,
    mut input: impl BufRead,
    mut output: impl Write,
) -> io::Result<()> {
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
                Ok(request) => dispatch(&request, &mut phase, &mut editor, path),
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

fn dispatch(
    request: &Value,
    phase: &mut Phase,
    editor: &mut ProjectEditor,
    path: Option<&Path>,
) -> Option<Value> {
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
            json!({"tools":tool_list(path.is_some())})
        }
        Some("tools/call") => match call(&params["name"], params.get("arguments"), editor, path) {
            Ok(result) => result,
            Err(message) => return Some(error(id, -32602, message)),
        },
        _ => return Some(error(id, -32601, "Unknown method")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

const MAX_EDITS: usize = 64;

fn tool_list(editable: bool) -> Vec<Value> {
    let empty = json!({"type":"object","properties":{},"additionalProperties":false});
    let mut tools = Vec::new();
    let mut add = |name: &str, description: &str, schema: &Value, read_only| {
        tools.push(
            json!({"name":name,"description":description,"inputSchema":schema,
            "annotations":{"readOnlyHint":read_only}}),
        );
    };
    add(SUMMARY, "Read the current project summary.", &empty, true);
    add(
        "get_main_pattern",
        "Read the main velocity grid. Rows match trackIds and steps start at zero.",
        &empty,
        true,
    );
    if editable {
        let step = json!({"type":"object","required":["track_id","step","velocity"],"additionalProperties":false,
            "properties":{"track_id":{"type":"integer","minimum":1},"step":{"type":"integer","minimum":0,"maximum":oxide_core::sequencer::STEPS-1},
            "velocity":{"type":"integer","minimum":0,"maximum":127}}});
        let batch = json!({"type":"object","required":["steps"],"additionalProperties":false,
            "properties":{"steps":{"type":"array","minItems":1,"maxItems":MAX_EDITS,"items":step}}});
        add(
            "set_main_steps",
            "Apply main pattern velocities atomically. Zero clears a step.",
            &batch,
            false,
        );
        add(
            "undo",
            "Undo the last edit batch in this session.",
            &empty,
            false,
        );
        add("redo", "Redo the last undone batch.", &empty, false);
        add(
            "save_project",
            "Replace the launch file with the current session project.",
            &empty,
            false,
        );
    }
    tools
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StepEdit {
    track_id: u64,
    step: usize,
    velocity: u8,
}

fn tool_result(data: Value, failed: bool) -> Value {
    json!({"isError":failed,"content":[{"type":"text","text":data.to_string()}],"structuredContent":data})
}

fn call(
    name: &Value,
    args: Option<&Value>,
    editor: &mut ProjectEditor,
    path: Option<&Path>,
) -> Result<Value, &'static str> {
    let name = name.as_str().ok_or("Expected a tool name")?;
    if name != "set_main_steps"
        && args.is_some_and(|v| !v.as_object().is_some_and(|a| a.is_empty()))
    {
        return Err("Expected an empty arguments object");
    }
    let project = editor.project();
    let data = match name {
        SUMMARY => summary(project),
        "get_main_pattern" => {
            json!({"trackIds":project.tracks.each_ref().map(|t|t.id),"pattern":project.pattern})
        }
        "set_main_steps" if path.is_some() => {
            let args = args
                .and_then(Value::as_object)
                .filter(|a| a.len() == 1)
                .ok_or("Expected only steps")?;
            let steps = args
                .get("steps")
                .and_then(Value::as_array)
                .filter(|s| (1..=MAX_EDITS).contains(&s.len()))
                .ok_or("Expected 1 to 64 step edits")?;
            let mut commands = Vec::with_capacity(steps.len());
            for step in steps {
                let StepEdit {
                    track_id,
                    step,
                    velocity,
                } = serde_json::from_value(step.clone())
                    .map_err(|_| "Expected integer track_id, step and velocity fields")?;
                commands.push(EditCommand::SetStep {
                    track_id,
                    step,
                    velocity,
                });
            }
            match editor.apply_batch(&commands) {
                Ok(changed) => json!({"changed":changed}),
                Err(err) => {
                    return Ok(tool_result(
                        json!({"code":"edit_rejected","message":err.to_string()}),
                        true,
                    ))
                }
            }
        }
        "undo" if path.is_some() => json!({"changed":editor.undo()}),
        "redo" if path.is_some() => json!({"changed":editor.redo()}),
        "save_project" => {
            let path = path.ok_or("Unknown tool")?;
            if let Err(err) = project.save(path) {
                return Ok(tool_result(
                    json!({"code":"save_failed","message":err.to_string()}),
                    true,
                ));
            }
            json!({"saved":true})
        }
        _ => return Err("Unknown tool"),
    };
    Ok(tool_result(data, false))
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
