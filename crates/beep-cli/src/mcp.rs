//! MCP(Model Context Protocol) 서버: `beep-gen mcp`
//!
//! AI 도구가 이 컴퓨터에서 beep-gen 을 직접 부를 수 있게 한다. 표준 입출력으로
//! 한 줄에 하나씩 JSON-RPC 2.0 메시지를 주고받는다(stdio 전송).
//!
//! 여기도 어댑터다: 요청을 읽어 공용 절차(`beep_native::job`)를 부르고 결과를 돌려준다.
//! 도구의 입력 명세는 코어의 파라미터 명세에서 만들어지므로, 코어에 파라미터를
//! 추가하면 MCP 도구에도 자동으로 나타난다.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use beep_core::{DEFAULT_INTRO_TEMPLATE, OutputFormat, ParamKind, TrackParams, param_specs};
use beep_native::job::{self, IntroRequest, JobRequest, MusicSource};
use beep_native::{fetch, tts};
use serde_json::{Map, Value, json};

/// 이 서버가 아는 프로토콜 버전(최신이 앞).
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
const SERVER_NAME: &str = "beep-gen";

const TOOL_CREATE: &str = "create_track";
const TOOL_VOICES: &str = "list_voices";

// JSON-RPC 오류 코드
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// 표준 입출력으로 MCP 서버를 실행한다. 입력이 끝나면 돌아온다.
pub fn serve_stdio() -> io::Result<()> {
    serve(io::stdin().lock(), io::stdout().lock())
}

/// 한 줄에 하나씩 메시지를 읽어 처리한다.
pub fn serve(reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(e) => {
                send(
                    &mut writer,
                    &error_response(Value::Null, PARSE_ERROR, &format!("JSON 해석 실패: {e}")),
                )?;
                continue;
            }
        };
        if let Some(response) = handle_message(&message, &mut writer)? {
            send(&mut writer, &response)?;
        }
    }
    Ok(())
}

fn send(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    // 메시지 안에 줄바꿈이 들어가면 안 되므로 한 줄로 내보낸다.
    writeln!(writer, "{message}")?;
    writer.flush()
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// 메시지 하나를 처리한다. 응답이 필요 없는 알림이면 `None`.
fn handle_message(message: &Value, writer: &mut impl Write) -> io::Result<Option<Value>> {
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // 우리가 보낸 요청이 없으므로 응답 메시지는 올 일이 없다. 형식이 틀린 메시지다.
        return Ok(message
            .get("id")
            .filter(|_| message.get("result").is_none() && message.get("error").is_none())
            .map(|id| error_response(id.clone(), INVALID_REQUEST, "method 가 없습니다")));
    };
    let Some(id) = message.get("id").cloned() else {
        // 알림(initialized, cancelled 등)에는 답하지 않는다.
        return Ok(None);
    };
    let params = message.get("params").cloned().unwrap_or(Value::Null);

    let result = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(&params, writer)?,
        other => Err((
            METHOD_NOT_FOUND,
            format!("지원하지 않는 요청입니다: {other}"),
        )),
    };
    Ok(Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_response(id, code, &message),
    }))
}

fn initialize(params: &Value) -> Value {
    // 클라이언트가 요청한 버전을 알면 그대로, 모르면 우리가 아는 최신 버전으로 답한다.
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|v| PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
        "instructions": "일정한 주기로 비프음이 울리는 오디오 파일을 만듭니다. \
    배경 음악(파일 경로 또는 오디오 파일의 직접 주소)을 주면 비프 템포에 맞춰 속도를 바꿔 함께 넣고, \
    맨 앞에 안내 음성(TTS)을 넣을 수 있습니다. 결과는 이 컴퓨터의 지정한 경로에 저장됩니다.",
    })
}

/// 도구 목록. `create_track` 의 입력 명세는 코어의 파라미터 명세에서 만든다.
fn tools() -> Value {
    let mut properties = Map::new();
    for spec in param_specs() {
        let mut property = Map::new();
        let when = match spec.when {
            beep_core::ParamWhen::Always => "",
            beep_core::ParamWhen::WithMusic => " 배경 음악이 있을 때만 쓰입니다.",
            beep_core::ParamWhen::WithoutMusic => {
                " 배경 음악이 있으면 쓰이지 않습니다(길이는 음악을 따릅니다)."
            }
        };
        property.insert(
            "description".into(),
            json!(format!(
                "{} ({}). {}{}",
                spec.label, spec.unit, spec.description, when
            )),
        );
        match spec.kind {
            ParamKind::Choice => {
                property.insert("type".into(), json!("integer"));
                let choices: Vec<i64> = spec.choices.iter().map(|c| *c as i64).collect();
                property.insert("enum".into(), json!(choices));
                property.insert("default".into(), json!(spec.default as i64));
            }
            ParamKind::Integer => {
                property.insert("type".into(), json!("integer"));
                property.insert("minimum".into(), json!(spec.min as i64));
                property.insert("maximum".into(), json!(spec.max as i64));
                property.insert("default".into(), json!(spec.default as i64));
            }
            ParamKind::Number => {
                property.insert("type".into(), json!("number"));
                property.insert("minimum".into(), json!(spec.min));
                property.insert("maximum".into(), json!(spec.max));
                property.insert("default".into(), json!(spec.default));
            }
        }
        properties.insert(spec.key.into(), Value::Object(property));
    }

    let formats: Vec<&str> = OutputFormat::ALL.iter().map(|f| f.extension()).collect();
    properties.insert(
        "output".into(),
        json!({
            "type": "string",
            "description": "저장할 파일의 절대 경로. 확장자(.mp3 또는 .wav)로 형식이 정해집니다.",
        }),
    );
    properties.insert(
        "music".into(),
        json!({
            "type": "string",
            "description": format!(
                "배경 음악: 이 컴퓨터의 파일 경로 또는 오디오 파일의 직접 주소(http/https, 예: https://.../song.mp3). \
비프 템포에 맞춰 속도를 바꿔 함께 넣습니다. MP3·WAV·FLAC·OGG·M4A. {}",
                fetch::STREAMING_NOT_SUPPORTED
            ),
        }),
    );
    properties.insert(
        "intro_text".into(),
        json!({
            "type": "string",
            "description": "맨 앞에 넣을 안내 음성(TTS)의 문구. {n} 은 주기(초), {m} 은 분할 수로 바뀝니다. \
빈 문자열이면 안내 음성을 넣지 않습니다.",
            "default": DEFAULT_INTRO_TEMPLATE,
        }),
    );
    properties.insert(
        "voice".into(),
        json!({
            "type": "string",
            "description": format!(
                "안내 음성의 목소리 이름(일부만 적어도 됨). 생략하면 문구의 언어에 맞게 고릅니다. 목록은 {TOOL_VOICES} 도구로 확인합니다."
            ),
        }),
    );
    properties.insert(
        "format".into(),
        json!({
            "type": "string",
            "enum": formats,
            "description": "출력 형식. 생략하면 output 의 확장자를 따릅니다.",
        }),
    );
    properties.insert(
        "overwrite".into(),
        json!({
            "type": "boolean",
            "description": "output 경로에 파일이 이미 있을 때 덮어쓸지. 기본은 덮어쓰지 않고 오류를 냅니다.",
            "default": false,
        }),
    );

    json!([
        {
            "name": TOOL_CREATE,
            "description": "일정한 주기로 비프음이 울리는 오디오 파일을 만들어 지정한 경로에 저장합니다. \
    배경 음악을 주면 비프 템포에 맞춰 속도를 바꿔 함께 넣고(길이는 음악을 따름), 맨 앞에 안내 음성을 넣습니다.",
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": ["output"],
                "additionalProperties": false,
            },
        },
        {
            "name": TOOL_VOICES,
            "description": "안내 음성(TTS)에 쓸 수 있는, 이 컴퓨터에 설치된 목소리 목록을 돌려줍니다.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
        },
    ])
}

type RpcResult = Result<Value, (i64, String)>;

fn tool_text(text: String, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn call_tool(params: &Value, writer: &mut impl Write) -> io::Result<RpcResult> {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return Ok(Err((INVALID_PARAMS, "도구 이름(name)이 없습니다".into())));
    };
    let arguments = match params.get("arguments") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(map)) => map.clone(),
        Some(_) => {
            return Ok(Err((INVALID_PARAMS, "arguments 는 객체여야 합니다".into())));
        }
    };
    let progress_token = params
        .get("_meta")
        .and_then(|meta| meta.get("progressToken"))
        .cloned();

    Ok(match name {
        TOOL_VOICES => Ok(list_voices()),
        TOOL_CREATE => {
            let mut write_error = None;
            let outcome = create_track(arguments, |percent, stage| {
                let Some(token) = &progress_token else { return };
                let notification = json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": {
                        "progressToken": token,
                        "progress": percent,
                        "total": 100,
                        "message": stage,
                    },
                });
                if let Err(e) = send(writer, &notification) {
                    write_error.get_or_insert(e);
                }
            });
            if let Some(e) = write_error {
                return Err(e);
            }
            // 도구 실행 실패는 프로토콜 오류가 아니라 "실패한 결과" 로 돌려준다.
            Ok(match outcome {
                Ok(summary) => tool_text(summary, false),
                Err(message) => tool_text(message, true),
            })
        }
        other => Err((INVALID_PARAMS, format!("없는 도구입니다: {other}"))),
    })
}

fn list_voices() -> Value {
    if !tts::is_supported() {
        return tool_text(
            "이 운영체제에서는 음성 합성(TTS)을 아직 지원하지 않습니다.".into(),
            true,
        );
    }
    match tts::list_voices() {
        Ok(voices) => tool_text(
            serde_json::to_string_pretty(&voices).unwrap_or_default(),
            false,
        ),
        Err(e) => tool_text(e.to_string(), true),
    }
}

fn take_string(arguments: &mut Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match arguments.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(format!("{key} 는 문자열이어야 합니다")),
    }
}

/// 음원을 만들어 저장하고, 결과 요약(JSON 문자열)을 돌려준다.
fn create_track(
    mut arguments: Map<String, Value>,
    on_progress: impl FnMut(u8, &str),
) -> Result<String, String> {
    let output = take_string(&mut arguments, "output")?
        .ok_or("output(저장할 파일의 절대 경로)이 필요합니다")?;
    let music = take_string(&mut arguments, "music")?;
    let intro_text = take_string(&mut arguments, "intro_text")?;
    let voice = take_string(&mut arguments, "voice")?;
    let format_name = take_string(&mut arguments, "format")?;
    let overwrite = match arguments.remove("overwrite") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => value,
        Some(_) => return Err("overwrite 는 true 또는 false 여야 합니다".into()),
    };

    // 남은 항목은 코어의 파라미터다. 정수 항목에 2.0 처럼 소수 꼴로 온 값은 정수로 바꿔 준다.
    for spec in param_specs() {
        if spec.kind == ParamKind::Number {
            continue;
        }
        if let Some(value) = arguments.get_mut(spec.key) {
            if let Some(number) = value.as_f64().filter(|n| n.fract() == 0.0 && *n >= 0.0) {
                *value = json!(number as u64);
            }
        }
    }
    let params: TrackParams = serde_json::from_value(Value::Object(arguments))
        .map_err(|e| format!("잘못된 설정값: {e}"))?;

    let output = PathBuf::from(output);
    let format = resolve_format(&output, format_name.as_deref())?;
    check_output_path(&output, overwrite)?;

    let request = JobRequest {
        params,
        format,
        music: music
            .as_deref()
            .filter(|m| !m.trim().is_empty())
            .map(MusicSource::parse),
        intro: Some(IntroRequest {
            template: intro_text.unwrap_or_else(|| DEFAULT_INTRO_TEMPLATE.to_owned()),
            voice: voice.filter(|v| !v.trim().is_empty()),
        }),
    };
    let result = job::run(&request, on_progress).map_err(|e| e.to_string())?;

    std::fs::write(&output, &result.bytes)
        .map_err(|e| format!("{} 에 저장하지 못했습니다: {e}", output.display()))?;
    let summary = json!({
        "output": output.display().to_string(),
        "format": format.extension(),
        "bytes": result.bytes.len(),
        "intro_seconds": result.intro_seconds,
        "music": result.music_report,
        "notices": result.notices,
    });
    Ok(serde_json::to_string_pretty(&summary).unwrap_or_default())
}

/// 출력 형식을 정한다. `format` 이 있으면 그것을, 없으면 파일 확장자를 따른다.
fn resolve_format(output: &Path, format: Option<&str>) -> Result<OutputFormat, String> {
    let from_extension = output
        .extension()
        .and_then(|e| e.to_str())
        .and_then(OutputFormat::from_extension);
    match format {
        Some(name) => {
            let wanted = OutputFormat::from_extension(name)
                .ok_or_else(|| format!("지원하지 않는 형식입니다: {name}"))?;
            if from_extension.is_some_and(|ext| ext != wanted) {
                return Err(format!(
                    "format({name})과 output 의 확장자가 서로 다릅니다: {}",
                    output.display()
                ));
            }
            Ok(wanted)
        }
        None => from_extension.ok_or_else(|| {
            format!(
                "output 의 확장자는 .mp3 또는 .wav 여야 합니다: {}",
                output.display()
            )
        }),
    }
}

/// 저장 경로가 쓸 수 있는 곳인지 확인한다.
fn check_output_path(output: &Path, overwrite: bool) -> Result<(), String> {
    if !output.is_absolute() {
        return Err(format!(
            "output 은 절대 경로여야 합니다(서버의 현재 폴더는 예측할 수 없습니다): {}",
            output.display()
        ));
    }
    if !output.parent().is_some_and(Path::is_dir) {
        return Err(format!("저장할 폴더가 없습니다: {}", output.display()));
    }
    if output.is_dir() {
        return Err(format!("output 이 폴더입니다: {}", output.display()));
    }
    if output.exists() && !overwrite {
        return Err(format!(
            "파일이 이미 있습니다. 덮어쓰려면 overwrite 를 true 로 주세요: {}",
            output.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 요청들을 차례로 보내고 서버가 내보낸 메시지들을 돌려준다.
    fn talk(requests: &[Value]) -> Vec<Value> {
        let input: String = requests.iter().map(|r| format!("{r}\n")).collect();
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output).unwrap();
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn call(id: u64, name: &str, arguments: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": { "name": name, "arguments": arguments } })
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("beep-gen-mcp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn initialize_negotiates_version_and_announces_tools() {
        let replies = talk(&[
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": { "protocolVersion": "2025-03-26", "capabilities": {} } }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize",
                    "params": { "protocolVersion": "1999-01-01" } }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" }),
        ]);
        // 알림에는 답하지 않으므로 응답은 3개다.
        assert_eq!(replies.len(), 3);
        assert_eq!(replies[0]["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(replies[0]["result"]["serverInfo"]["name"], "beep-gen");
        assert!(replies[0]["result"]["capabilities"]["tools"].is_object());
        assert_eq!(
            replies[1]["result"]["protocolVersion"],
            PROTOCOL_VERSIONS[0]
        );
        assert_eq!(replies[2]["result"], json!({}));
    }

    #[test]
    fn tool_schema_follows_core_param_specs() {
        let replies = talk(&[json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })]);
        let tools = replies[0]["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
        let create = &tools[0];
        assert_eq!(create["name"], TOOL_CREATE);
        let properties = create["inputSchema"]["properties"].as_object().unwrap();
        for spec in param_specs() {
            let property = properties
                .get(spec.key)
                .unwrap_or_else(|| panic!("{} 가 도구 명세에 없습니다", spec.key));
            assert_eq!(
                property["default"].as_f64().unwrap(),
                spec.default,
                "{}",
                spec.key
            );
        }
        assert_eq!(create["inputSchema"]["required"], json!(["output"]));
        assert_eq!(properties["interval"]["type"], "number");
        assert_eq!(properties["subdivision"]["type"], "integer");
        assert_eq!(
            properties["sample-rate"]["enum"],
            json!([32_000, 44_100, 48_000])
        );
    }

    #[test]
    fn create_track_writes_the_file_and_reports_progress() {
        let output = temp_path("ok.wav");
        let _ = std::fs::remove_file(&output);
        let replies = talk(&[json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": {
                "name": TOOL_CREATE,
                "arguments": { "output": output, "interval": 1.4, "subdivision": 2.0,
                               "length": 2, "intro_text": "" },
                "_meta": { "progressToken": "p1" },
            },
        })]);
        let (progress, response) = replies.split_at(replies.len() - 1);
        assert!(!progress.is_empty(), "진행 알림이 있어야 합니다");
        assert!(progress.iter().all(
            |m| m["method"] == "notifications/progress" && m["params"]["progressToken"] == "p1"
        ));
        assert_eq!(progress.last().unwrap()["params"]["progress"], 100);

        let result = &response[0]["result"];
        assert_eq!(result["isError"], false, "{result}");
        let summary: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(summary["format"], "wav");
        assert_eq!(summary["intro_seconds"], 0.0);
        assert_eq!(
            std::fs::metadata(&output).unwrap().len(),
            44 + 2 * 44_100 * 2
        );
        assert_eq!(summary["bytes"], 44 + 2 * 44_100 * 2);
    }

    #[test]
    fn existing_file_is_kept_unless_overwrite() {
        let output = temp_path("keep.wav");
        std::fs::write(&output, b"original").unwrap();
        let arguments = json!({ "output": output, "length": 1, "intro_text": "" });

        let replies = talk(&[call(1, TOOL_CREATE, arguments.clone())]);
        assert_eq!(replies[0]["result"]["isError"], true);
        assert_eq!(std::fs::read(&output).unwrap(), b"original");

        let mut with_overwrite = arguments;
        with_overwrite["overwrite"] = json!(true);
        let replies = talk(&[call(2, TOOL_CREATE, with_overwrite)]);
        assert_eq!(replies[0]["result"]["isError"], false, "{}", replies[0]);
        assert!(std::fs::metadata(&output).unwrap().len() > 1000);
    }

    #[test]
    fn bad_arguments_are_tool_errors_with_reasons() {
        let good = temp_path("unused.wav");
        let cases = [
            (json!({ "length": 1 }), "output"),
            (json!({ "output": "relative.wav" }), "절대 경로"),
            (json!({ "output": temp_path("x.ogg") }), "확장자"),
            (json!({ "output": good, "format": "mp3" }), "서로 다릅니다"),
            (json!({ "output": good, "subdivision": 0 }), "subdivision"),
            (
                json!({ "output": good, "no-such-option": 1 }),
                "no-such-option",
            ),
            (
                json!({ "output": good, "music": "https://youtu.be/x", "intro_text": "" }),
                "포크",
            ),
        ];
        for (id, (arguments, expected)) in cases.into_iter().enumerate() {
            let replies = talk(&[call(id as u64, TOOL_CREATE, arguments.clone())]);
            let result = &replies[0]["result"];
            assert_eq!(result["isError"], true, "{arguments}");
            let text = result["content"][0]["text"].as_str().unwrap();
            assert!(text.contains(expected), "{arguments} → {text}");
        }
        assert!(!good.exists(), "실패한 요청이 파일을 만들면 안 됩니다");
    }

    #[test]
    fn protocol_errors_use_json_rpc_codes() {
        let replies = talk(&[
            json!({ "jsonrpc": "2.0", "id": 1, "method": "no/such" }),
            call(2, "no_such_tool", json!({})),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {} }),
        ]);
        assert_eq!(replies[0]["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(replies[1]["error"]["code"], INVALID_PARAMS);
        assert_eq!(replies[2]["error"]["code"], INVALID_PARAMS);

        let mut output = Vec::new();
        serve("this is not json\n".as_bytes(), &mut output).unwrap();
        let reply: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(reply["error"]["code"], PARSE_ERROR);
        assert_eq!(reply["id"], Value::Null);
    }
}
