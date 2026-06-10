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
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verdicts {
    pub verdicts: Vec<Verdict>,
}

#[derive(Clone)]
pub struct Generator {
    pub claude_bin: String,
    pub codex_bin: Option<String>,
    pub scratch_dir: PathBuf,
}

pub const COURSE_PROMPT: &str = include_str!("../prompts/course.txt");
pub const QUIZ_PROMPT: &str = include_str!("../prompts/quiz.txt");
pub const GRADE_PROMPT: &str = include_str!("../prompts/grade.txt");

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
    pub fn new(claude_bin: String, codex_bin: Option<String>, scratch_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&scratch_dir);
        Self { claude_bin, codex_bin, scratch_dir }
    }

    pub async fn generate_course(&self, title: &str, category: &str) -> Result<(GeneratedCourse, String)> {
        let prompt = COURSE_PROMPT
            .replace("{{TITLE}}", title)
            .replace("{{CATEGORY}}", category);
        match self
            .run_with_fallback::<GeneratedCourse>(&prompt, true, Duration::from_secs(480))
            .await
        {
            Ok((course, source)) => Ok((course, source)),
            Err(e) => {
                log::warn!("course generation failed entirely, using bundled fallback: {e}");
                let fb = pick_fallback(title);
                Ok((
                    GeneratedCourse {
                        title: fb.title,
                        markdown: fb.markdown,
                        resources: fb.resources,
                        key_takeaways: fb.key_takeaways,
                    },
                    "fallback".into(),
                ))
            }
        }
    }

    pub async fn generate_quiz(&self, course_markdown: &str) -> Result<(Vec<GeneratedQuestion>, String)> {
        let prompt = QUIZ_PROMPT.replace("{{COURSE}}", course_markdown);
        match self
            .run_with_fallback::<GeneratedQuiz>(&prompt, false, Duration::from_secs(180))
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

    /// Grade free-text answers. On total failure returns None — caller shows self-assess mode.
    pub async fn grade(&self, items: &[GradeItem]) -> Option<Vec<Verdict>> {
        if items.is_empty() {
            return Some(vec![]);
        }
        let items_json = serde_json::to_string_pretty(items).ok()?;
        let prompt = GRADE_PROMPT.replace("{{ITEMS}}", &items_json);
        match self
            .run_with_fallback::<Verdicts>(&prompt, false, Duration::from_secs(120))
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
    ) -> Result<(T, String)> {
        let mut last_err: Option<GenError> = None;
        for attempt in 0..2 {
            match self.run_claude(prompt, web_tools, timeout).await {
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

    async fn run_claude(&self, prompt: &str, web_tools: bool, timeout: Duration) -> Result<String> {
        let mut cmd = Command::new(&self.claude_bin);
        cmd.arg("-p")
            .arg("--output-format")
            .arg("json")
            .arg("--model")
            .arg("sonnet")
            .arg("--max-turns")
            .arg("25")
            .current_dir(&self.scratch_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if web_tools {
            cmd.arg("--allowedTools").arg("WebSearch WebFetch");
        } else {
            cmd.arg("--allowedTools").arg("");
        }
        cmd.arg("--disallowedTools").arg("Bash Edit Write NotebookEdit");
        cmd.arg(prompt);
        let raw = run_capture(cmd, timeout).await?;
        // claude --output-format json wraps the reply in {"result": "..."} among other fields
        if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(result) = envelope.get("result").and_then(|r| r.as_str()) {
                return Ok(result.to_string());
            }
        }
        Ok(raw)
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
        let out = self.run_claude(&prompt, false, Duration::from_secs(120)).await?;
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
