use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("agent binary not found")]
    NoBinary,
    #[error("agent timed out after {0:?}")]
    Timeout(Duration),
    #[error("agent exited with status {0}: {1}")]
    BadExit(i32, String),
    #[error("could not parse agent output: {0}")]
    Parse(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, GenError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub title: String,
    pub url: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedCourse {
    pub title: String,
    pub markdown: String,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub key_takeaways: Vec<String>,
    /// 3 MCQs that gate early exit from the reader (optional in the payload;
    /// generated on demand if the model omits them).
    #[serde(default)]
    pub exit_questions: Vec<ExitCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitCheck {
    pub prompt: String,
    pub choices: Vec<String>,
    pub correct_answer: String,
    #[serde(default)]
    pub explanation: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExitChecks {
    pub questions: Vec<ExitCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedQuestion {
    pub prompt: String,
    pub kind: String,
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    pub correct_answer: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeneratedQuiz {
    pub questions: Vec<GeneratedQuestion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GradeItem {
    pub id: i64,
    pub concept: String,
    pub question: String,
    pub model_answer: String,
    pub user_answer: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verdict {
    pub id: i64,
    pub correct: bool,
    #[serde(default)]
    pub feedback: String,
    /// Teacher's private observation about the student on this concept.
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verdicts {
    pub verdicts: Vec<Verdict>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionPlan {
    pub session_type: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Clone)]
pub struct Generator {
    pub claude_bin: String,
    pub codex_bin: Option<String>,
    pub scratch_dir: PathBuf,
    /// Primary model for course generation (the expensive, quality-bound call).
    /// Quiz and grading stay on sonnet: rubric-bound, latency-sensitive.
    pub model: String,
    /// Live agent-activity lines for the UI (gen:log). None in tests.
    pub log_tx: Option<tokio::sync::broadcast::Sender<String>>,
}

pub const COURSE_PROMPT: &str = include_str!("../prompts/course.txt");
pub const QUIZ_PROMPT: &str = include_str!("../prompts/quiz.txt");
pub const GRADE_PROMPT: &str = include_str!("../prompts/grade.txt");
pub const TEACHER_PROMPT: &str = include_str!("../prompts/teacher.txt");
pub const PLAN_PROMPT: &str = include_str!("../prompts/plan.txt");
pub const EXIT_PROMPT: &str = include_str!("../prompts/exit.txt");
pub const AUDIO_PROMPT: &str = include_str!("../prompts/audio.txt");

/// Model for quiz/grade/repair calls regardless of the configured primary.
const SMALL_MODEL: &str = "sonnet";

/// Prepend the Teacher role + dossier to a task prompt. Empty dossier
/// (fresh install, day 1) skips the preamble entirely.
fn with_teacher(dossier: &str, task_prompt: &str) -> String {
    if dossier.trim().is_empty() {
        return task_prompt.to_string();
    }
    format!("{}\n\n{}", TEACHER_PROMPT.replace("{{DOSSIER}}", dossier.trim()), task_prompt)
}

pub const FALLBACK_COURSES: &[&str] = &[
    include_str!("../seed/fallback_courses/rate-limiting.json"),
    include_str!("../seed/fallback_courses/caching-strategies.json"),
    include_str!("../seed/fallback_courses/consistent-hashing.json"),
];

#[derive(Debug, Clone, Deserialize)]
pub struct FallbackCourse {
    pub slug: String,
    pub title: String,
    pub markdown: String,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub key_takeaways: Vec<String>,
    pub questions: Vec<GeneratedQuestion>,
}

impl Generator {
    pub fn new(
        claude_bin: String,
        codex_bin: Option<String>,
        scratch_dir: PathBuf,
        model: String,
        log_tx: Option<tokio::sync::broadcast::Sender<String>>,
    ) -> Self {
        let _ = std::fs::create_dir_all(&scratch_dir);
        Self { claude_bin, codex_bin, scratch_dir, model, log_tx }
    }

    fn log(&self, msg: impl Into<String>) {
        if let Some(tx) = &self.log_tx {
            let _ = tx.send(msg.into());
        }
    }

    pub async fn generate_course(
        &self,
        title: &str,
        category: &str,
        dossier: &str,
    ) -> Result<(GeneratedCourse, String)> {
        let prompt = with_teacher(
            dossier,
            &COURSE_PROMPT
                .replace("{{TITLE}}", title)
                .replace("{{CATEGORY}}", category),
        );
        match self
            .run_with_fallback::<GeneratedCourse>(&prompt, true, Duration::from_secs(720), &self.model)
            .await
        {
            Ok((course, source)) => Ok((course, source)),
            Err(e) => {
                log::warn!("course generation failed entirely, using bundled fallback: {e}");
                let fb = pick_fallback(title);
                // Bundled courses carry quiz MCQs — reuse 3 as the exit check.
                let exit_questions = fb
                    .questions
                    .iter()
                    .filter(|q| q.kind == "mcq" && q.choices.is_some())
                    .take(3)
                    .map(|q| ExitCheck {
                        prompt: q.prompt.clone(),
                        choices: q.choices.clone().unwrap_or_default(),
                        correct_answer: q.correct_answer.clone(),
                        explanation: q.explanation.clone(),
                    })
                    .collect();
                Ok((
                    GeneratedCourse {
                        title: fb.title,
                        markdown: fb.markdown,
                        resources: fb.resources,
                        key_takeaways: fb.key_takeaways,
                        exit_questions,
                    },
                    "fallback".into(),
                ))
            }
        }
    }

    pub async fn generate_quiz(
        &self,
        course_markdown: &str,
        dossier: &str,
    ) -> Result<(Vec<GeneratedQuestion>, String)> {
        let prompt = with_teacher(dossier, &QUIZ_PROMPT.replace("{{COURSE}}", course_markdown));
        match self
            .run_with_fallback::<GeneratedQuiz>(&prompt, false, Duration::from_secs(180), SMALL_MODEL)
            .await
        {
            Ok((quiz, source)) => Ok((quiz.questions, source)),
            Err(e) => {
                log::warn!("quiz generation failed entirely, using bundled fallback: {e}");
                let fb = pick_fallback("");
                Ok((fb.questions, "fallback".into()))
            }
        }
    }

    /// Two-host dialogue script for audio-lesson mode. Spoken-register
    /// rewrite of the course; rendered or speech-synthesized by the caller.
    pub async fn generate_audio_script(
        &self,
        course_markdown: &str,
    ) -> Result<Vec<crate::audio::ScriptLine>> {
        let prompt = AUDIO_PROMPT.replace("{{COURSE}}", course_markdown);
        let (script, _) = self
            .run_with_fallback::<crate::audio::GeneratedScript>(
                &prompt,
                false,
                Duration::from_secs(300),
                SMALL_MODEL,
            )
            .await?;
        Ok(script.lines)
    }

    /// On-demand exit check for a course generated before this feature
    /// existed (or when the course payload omitted them). Small, fast call.
    pub async fn generate_exit_quiz(&self, course_markdown: &str) -> Result<Vec<ExitCheck>> {
        let prompt = EXIT_PROMPT.replace("{{COURSE}}", course_markdown);
        let (checks, _) = self
            .run_with_fallback::<ExitChecks>(&prompt, false, Duration::from_secs(120), SMALL_MODEL)
            .await?;
        Ok(checks.questions)
    }

    /// Ask the Teacher to choose tomorrow's session type. `eligible` describes
    /// whether pop_quiz is currently allowed (guardrails re-checked by caller).
    /// Failure falls back to a lesson day — planning can never block.
    pub async fn plan_day(&self, dossier: &str, eligible: bool) -> SessionPlan {
        let prompt = with_teacher(
            dossier,
            &PLAN_PROMPT.replace("{{ELIGIBLE}}", if eligible { "yes" } else { "no" }),
        );
        match self
            .run_with_fallback::<SessionPlan>(&prompt, false, Duration::from_secs(90), SMALL_MODEL)
            .await
        {
            Ok((plan, _)) => plan,
            Err(e) => {
                log::warn!("day planning failed, defaulting to lesson: {e}");
                SessionPlan { session_type: "lesson".into(), reason: String::new() }
            }
        }
    }

    /// Grade free-text answers. On total failure returns None — caller shows self-assess mode.
    pub async fn grade(&self, items: &[GradeItem]) -> Option<Vec<Verdict>> {
        if items.is_empty() {
            return Some(vec![]);
        }
        let items_json = serde_json::to_string_pretty(items).ok()?;
        let prompt = GRADE_PROMPT.replace("{{ITEMS}}", &items_json);
        match self
            .run_with_fallback::<Verdicts>(&prompt, false, Duration::from_secs(120), SMALL_MODEL)
            .await
        {
            Ok((v, _)) => Some(v.verdicts),
            Err(e) => {
                log::warn!("grading failed, falling back to self-assess: {e}");
                None
            }
        }
    }

    async fn run_with_fallback<T: serde::de::DeserializeOwned>(
        &self,
        prompt: &str,
        web_tools: bool,
        timeout: Duration,
        model: &str,
    ) -> Result<(T, String)> {
        let mut last_err: Option<GenError> = None;
        for attempt in 0..2 {
            // Retry downgrades to the small model: cheaper, and routes around
            // primary-model availability/timeout issues.
            let attempt_model = if attempt == 0 { model } else { SMALL_MODEL };
            match self.run_claude(prompt, web_tools, timeout, attempt_model).await {
                Ok(raw) => match parse_json_payload::<T>(&raw) {
                    Ok(v) => return Ok((v, "claude".into())),
                    Err(_) => match self.repair_json::<T>(&raw).await {
                        Ok(v) => return Ok((v, "claude".into())),
                        Err(e) => last_err = Some(e),
                    },
                },
                Err(e) => {
                    log::warn!("claude attempt {attempt} failed: {e}");
                    last_err = Some(e);
                }
            }
        }
        if let Some(codex) = &self.codex_bin {
            match self.run_codex(codex, prompt, timeout).await {
                Ok(raw) => {
                    if let Ok(v) = parse_json_payload::<T>(&raw) {
                        return Ok((v, "codex".into()));
                    }
                }
                Err(e) => {
                    log::warn!("codex fallback failed: {e}");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or(GenError::NoBinary))
    }

    fn claude_cmd(&self, prompt: &str, web_tools: bool, model: &str, stream: bool) -> Command {
        let mut cmd = Command::new(&self.claude_bin);
        cmd.arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg(if stream { "stream-json" } else { "json" })
            .arg("--model")
            .arg(model)
            .arg("--max-turns")
            .arg("25")
            .current_dir(&self.scratch_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stream {
            // print-mode stream-json requires --verbose
            cmd.arg("--verbose");
        }
        if web_tools {
            cmd.arg("--allowedTools").arg("WebSearch,WebFetch");
        }
        cmd.arg("--disallowedTools").arg("Bash,Edit,Write,NotebookEdit");
        cmd
    }

    async fn run_claude(&self, prompt: &str, web_tools: bool, timeout: Duration, model: &str) -> Result<String> {
        // Long calls (course, audio script) stream so the UI can show the
        // agent working; short rubric calls stay on the simple JSON envelope.
        if timeout >= Duration::from_secs(240) && self.log_tx.is_some() {
            match self.run_claude_stream(prompt, web_tools, timeout, model).await {
                Ok(out) => return Ok(out),
                Err(e) => {
                    // e.g. an older CLI without stream-json — degrade silently.
                    log::warn!("streaming run failed ({e}), retrying buffered");
                }
            }
        }
        let cmd = self.claude_cmd(prompt, web_tools, model, false);
        let raw = run_capture(cmd, timeout).await?;
        // claude --output-format json wraps the reply in {"result": "..."} among other fields
        if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(result) = envelope.get("result").and_then(|r| r.as_str()) {
                return Ok(result.to_string());
            }
        }
        Ok(raw)
    }

    /// stream-json variant: emits a gen:log line per agent event (web search
    /// queries, fetches, drafting turns) and returns the final result text.
    async fn run_claude_stream(
        &self,
        prompt: &str,
        web_tools: bool,
        timeout: Duration,
        model: &str,
    ) -> Result<String> {
        use tokio::io::{AsyncBufReadExt, BufReader};
        self.log(format!("spawn: agent · model {model}"));
        let mut child = self
            .claude_cmd(prompt, web_tools, model, true)
            .spawn()
            .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { GenError::NoBinary } else { GenError::Io(e) })?;
        let stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        // Drain stderr concurrently so the child can't block on a full pipe.
        let err_task = tokio::spawn(async move {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf).await;
            buf
        });

        let me = self.clone();
        let read_fut = async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut result: Option<String> = None;
            let mut drafted = 0usize;
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                match v.get("type").and_then(|t| t.as_str()) {
                    Some("assistant") => {
                        for block in v["message"]["content"].as_array().unwrap_or(&vec![]) {
                            match block.get("type").and_then(|t| t.as_str()) {
                                Some("tool_use") => {
                                    let name = block["name"].as_str().unwrap_or("tool");
                                    let detail = block["input"]["query"]
                                        .as_str()
                                        .or_else(|| block["input"]["url"].as_str())
                                        .unwrap_or("");
                                    me.log(format!("tool: {name} {}", detail.chars().take(80).collect::<String>()));
                                }
                                Some("text") => {
                                    let n = block["text"].as_str().map(|t| t.len()).unwrap_or(0);
                                    drafted += n;
                                    if n > 200 {
                                        me.log(format!("draft: {} chars written", drafted));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some("result") => {
                        result = v["result"].as_str().map(|s| s.to_string());
                    }
                    _ => {}
                }
            }
            let status = child.wait().await?;
            Ok::<(Option<String>, std::process::ExitStatus), std::io::Error>((result, status))
        };

        match tokio::time::timeout(timeout, read_fut).await {
            Ok(Ok((Some(result), status))) if status.success() => {
                self.log(format!("done: agent returned {} chars", result.len()));
                Ok(result)
            }
            Ok(Ok((_, status))) => {
                let err = err_task.await.unwrap_or_default();
                self.log("fail: agent exited without a result".to_string());
                Err(GenError::BadExit(status.code().unwrap_or(-1), err.chars().take(2000).collect()))
            }
            Ok(Err(e)) => Err(GenError::Io(e)),
            Err(_) => {
                self.log(format!("fail: agent timed out after {}s", timeout.as_secs()));
                Err(GenError::Timeout(timeout))
            }
        }
    }

    async fn run_codex(&self, codex_bin: &str, prompt: &str, timeout: Duration) -> Result<String> {
        let mut cmd = Command::new(codex_bin);
        cmd.arg("exec")
            .arg("--skip-git-repo-check")
            .arg(prompt)
            .current_dir(&self.scratch_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        run_capture(cmd, timeout).await
    }

    async fn repair_json<T: serde::de::DeserializeOwned>(&self, raw: &str) -> Result<T> {
        let truncated: String = raw.chars().take(60_000).collect();
        let prompt = format!(
            "The following text was supposed to be a single valid JSON object but is malformed or wrapped in prose. \
             Output ONLY the corrected JSON object inside one ```json fenced block, changing as little as possible:\n\n{truncated}"
        );
        let out = self.run_claude(&prompt, false, Duration::from_secs(120), SMALL_MODEL).await?;
        parse_json_payload::<T>(&out)
    }
}

async fn run_capture(mut cmd: Command, timeout: Duration) -> Result<String> {
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            GenError::NoBinary
        } else {
            GenError::Io(e)
        }
    })?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let read_fut = async {
        let mut out = String::new();
        let mut err = String::new();
        let _ = stdout.read_to_string(&mut out).await;
        let _ = stderr.read_to_string(&mut err).await;
        let status = child.wait().await?;
        Ok::<(String, String, std::process::ExitStatus), std::io::Error>((out, err, status))
    };
    match tokio::time::timeout(timeout, read_fut).await {
        Ok(Ok((out, err, status))) => {
            if status.success() {
                Ok(out)
            } else {
                Err(GenError::BadExit(
                    status.code().unwrap_or(-1),
                    err.chars().take(2000).collect(),
                ))
            }
        }
        Ok(Err(e)) => Err(GenError::Io(e)),
        Err(_) => Err(GenError::Timeout(timeout)),
    }
}

/// Extract a JSON object from raw model output: tries fenced ```json block, then
/// first-{ to last-} slice, then the raw string itself.
pub fn parse_json_payload<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    if let Some(start) = raw.find("```json") {
        let after = &raw[start + 7..];
        // Use the LAST closing fence: course markdown legitimately contains
        // embedded ``` blocks inside the JSON string.
        if let Some(end) = after.rfind("```") {
            let candidate = after[..end].trim();
            if let Ok(v) = serde_json::from_str::<T>(candidate) {
                return Ok(v);
            }
        }
        if let Some(end) = after.find("```") {
            let candidate = after[..end].trim();
            if let Ok(v) = serde_json::from_str::<T>(candidate) {
                return Ok(v);
            }
        }
    }
    if let (Some(start), Some(end)) = (raw.find('{'), raw.rfind('}')) {
        if end > start {
            if let Ok(v) = serde_json::from_str::<T>(&raw[start..=end]) {
                return Ok(v);
            }
        }
    }
    serde_json::from_str::<T>(raw.trim())
        .map_err(|e| GenError::Parse(format!("{e}; head: {}", raw.chars().take(300).collect::<String>())))
}

pub fn pick_fallback(preferred_title: &str) -> FallbackCourse {
    let all: Vec<FallbackCourse> = FALLBACK_COURSES
        .iter()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect();
    let lower = preferred_title.to_lowercase();
    if let Some(m) = all.iter().find(|c| lower.contains(&c.slug.replace('-', " ")) || lower.contains(&c.slug)) {
        return m.clone();
    }
    let idx = (chrono::Utc::now().timestamp() as usize / 86_400) % all.len().max(1);
    all.into_iter().nth(idx).expect("bundled fallback courses present")
}
