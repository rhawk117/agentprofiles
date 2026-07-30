use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SUBAGENT_TOOL_NAME: &str = "Task";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActivityCounters {
    pub tool_calls: u64,
    pub assistant_turns: u64,
    pub tool_errors: u64,
    pub subagents: u64,
    pub byte_offset: u64,
}

impl ActivityCounters {
    #[must_use]
    pub fn advanced_from(&self, transcript_path: &Path) -> Self {
        let Ok(file) = File::open(transcript_path) else {
            return *self;
        };
        let Ok(metadata) = file.metadata() else {
            return *self;
        };
        let file_length = metadata.len();

        let mut counters = if file_length < self.byte_offset {
            Self::default()
        } else {
            *self
        };

        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(counters.byte_offset)).is_err() {
            return counters;
        }

        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(bytes_read) => {
                    if !line.ends_with('\n') {
                        break;
                    }
                    counters.byte_offset = counters
                        .byte_offset
                        .saturating_add(bytes_read.try_into().unwrap_or(0));
                    counters.absorb_line(&line);
                }
            }
        }
        counters
    }

    fn absorb_line(&mut self, raw_line: &str) {
        let Ok(entry) = serde_json::from_str::<Value>(raw_line.trim()) else {
            return;
        };
        let Some(message) = entry.get("message") else {
            return;
        };
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if role == "assistant" {
            self.assistant_turns = self.assistant_turns.saturating_add(1);
        }
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            return;
        };
        for block in blocks {
            self.absorb_block(block);
        }
    }

    fn absorb_block(&mut self, block: &Value) {
        match block.get("type").and_then(Value::as_str) {
            Some("tool_use") => {
                self.tool_calls = self.tool_calls.saturating_add(1);
                if block.get("name").and_then(Value::as_str) == Some(SUBAGENT_TOOL_NAME) {
                    self.subagents = self.subagents.saturating_add(1);
                }
            }
            Some("tool_result")
                if block
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false) =>
            {
                self.tool_errors = self.tool_errors.saturating_add(1);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_transcript(name: &str, lines: &[&str]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut file = File::create(&path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        path
    }

    const ASSISTANT_WITH_TWO_TOOLS: &str = concat!(
        r#"{"type":"assistant","message":{"role":"assistant","content":"#,
        r#"[{"type":"tool_use","name":"Read"},{"type":"tool_use","name":"Task"}]}}"#
    );
    const USER_WITH_ERROR: &str = concat!(
        r#"{"type":"user","message":{"role":"user","content":"#,
        r#"[{"type":"tool_result","is_error":true}]}}"#
    );
    const PLAIN_ASSISTANT: &str =
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text"}]}}"#;

    #[test]
    fn counts_tool_calls_turns_errors_and_subagents() {
        let path = write_transcript(
            "instrumentline-transcript-basic.jsonl",
            &[ASSISTANT_WITH_TWO_TOOLS, USER_WITH_ERROR, PLAIN_ASSISTANT],
        );
        let counters = ActivityCounters::default().advanced_from(&path);
        assert_eq!(counters.tool_calls, 2);
        assert_eq!(counters.subagents, 1);
        assert_eq!(counters.tool_errors, 1);
        assert_eq!(counters.assistant_turns, 2);
        drop(std::fs::remove_file(&path));
    }

    #[test]
    fn resuming_only_reads_the_appended_tail() {
        let path = write_transcript(
            "instrumentline-transcript-resume.jsonl",
            &[ASSISTANT_WITH_TWO_TOOLS],
        );
        let first = ActivityCounters::default().advanced_from(&path);
        assert_eq!(first.tool_calls, 2);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{PLAIN_ASSISTANT}").unwrap();
        drop(file);

        let second = first.advanced_from(&path);
        assert_eq!(second.tool_calls, 2);
        assert_eq!(second.assistant_turns, 2);
        assert!(second.byte_offset > first.byte_offset);
        drop(std::fs::remove_file(&path));
    }

    #[test]
    fn a_truncated_transcript_resets_the_counters() {
        let path = write_transcript(
            "instrumentline-transcript-truncate.jsonl",
            &[ASSISTANT_WITH_TWO_TOOLS, PLAIN_ASSISTANT],
        );
        let first = ActivityCounters::default().advanced_from(&path);
        assert!(first.byte_offset > 0);

        let rewritten = write_transcript("instrumentline-transcript-truncate.jsonl", &[]);
        let second = first.advanced_from(&rewritten);
        assert_eq!(second.tool_calls, 0);
        assert_eq!(second.byte_offset, 0);
        drop(std::fs::remove_file(&path));
    }

    #[test]
    fn malformed_lines_are_skipped_without_losing_the_rest() {
        let path = write_transcript(
            "instrumentline-transcript-malformed.jsonl",
            &["{not json", "", ASSISTANT_WITH_TWO_TOOLS],
        );
        let counters = ActivityCounters::default().advanced_from(&path);
        assert_eq!(counters.tool_calls, 2);
        drop(std::fs::remove_file(&path));
    }

    #[test]
    fn a_partial_final_line_is_left_for_the_next_read() {
        let path = std::env::temp_dir().join("instrumentline-transcript-partial.jsonl");
        let mut file = File::create(&path).unwrap();
        writeln!(file, "{PLAIN_ASSISTANT}").unwrap();
        write!(file, r#"{{"type":"assistant","message":{{"role":"assis"#).unwrap();
        drop(file);

        let counters = ActivityCounters::default().advanced_from(&path);
        assert_eq!(counters.assistant_turns, 1);
        drop(std::fs::remove_file(&path));
    }

    #[test]
    fn a_missing_transcript_leaves_counters_untouched() {
        let previous = ActivityCounters {
            tool_calls: 7,
            ..ActivityCounters::default()
        };
        let counters = previous.advanced_from(Path::new("/nonexistent/transcript.jsonl"));
        assert_eq!(counters, previous);
    }
}
