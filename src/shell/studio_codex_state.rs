use super::*;

impl StudioShell {
    pub(crate) fn set_codex_project_root(&mut self, project_root: PathBuf) {
        self.codex_project_root = project_root;
    }

    pub(crate) fn take_codex_chat_open_request(&mut self) -> bool {
        std::mem::take(&mut self.codex_chat_open_requested)
    }

    pub(crate) fn take_codex_chat_send_request(&mut self) -> Option<CodexChatSendRequest> {
        self.codex_chat_send_requested.take()
    }

    pub(crate) fn submit_codex_chat(&mut self) {
        let message = self.codex_chat_draft.trim().to_owned();
        if message.is_empty() {
            return;
        }
        self.codex_chat_messages.push(CodexChatMessage {
            role: CodexChatRole::User,
            text: message.clone(),
        });
        self.codex_chat_draft.clear();
        self.codex_live_excerpt.clear();
        self.codex_live_pending_excerpt.clear();
        self.codex_live_excerpt_queue.clear();
        self.codex_live_last_published_at = None;
        self.codex_live_last_received_at = None;
        self.codex_live_needs_separator = false;
        self.codex_live_in_code_block = false;
        self.codex_activity = CodexActivity::Thinking;
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
        self.codex_chat_error = None;
        self.codex_chat_send_requested = Some(CodexChatSendRequest {
            message,
            model: self.codex_chat_model,
            reasoning_effort: self.codex_chat_reasoning_effort,
        });
    }

    pub(crate) fn set_codex_chat_ready(&mut self) {
        self.codex_chat_ready = true;
        self.codex_chat_error = None;
    }

    pub(crate) fn set_codex_chat_delta(&mut self, delta: String) {
        self.append_codex_live_excerpt(&delta);
        self.set_codex_activity(self.codex_activity.after_agent_progress());
    }

    pub(crate) fn set_codex_work_status(&mut self, status: CodexWorkStatus) {
        if !self.codex_activity.is_cancellable() {
            return;
        }
        let activity = match status {
            CodexWorkStatus::Thinking => CodexActivity::Thinking,
            CodexWorkStatus::Editing => CodexActivity::Editing,
            CodexWorkStatus::Checking => CodexActivity::Checking,
            CodexWorkStatus::Working => CodexActivity::Working,
        };
        self.set_codex_activity(activity);
    }

    pub(crate) fn set_codex_chat_message(&mut self, text: String) {
        // An agent-message item can complete while the turn continues with
        // more tool work. Only turn/completed advances Studio to rebuilding.
        self.append_codex_live_excerpt(&text);
        self.set_codex_activity(self.codex_activity.after_agent_progress());
    }

    pub(crate) fn set_codex_chat_completed(&mut self) {
        self.set_codex_activity(CodexActivity::Rebuilding);
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
    }

    pub(crate) fn set_codex_preview_rebuilt(&mut self) {
        self.finish_codex_activity("Done — preview rebuilt and playing.");
    }

    pub(crate) fn set_codex_preview_rebuild_failed(&mut self, message: &str) {
        self.finish_codex_activity("The change was made, but the preview could not be rebuilt.");
        self.codex_chat_error = Some(format!("Rebuild failed: {message}"));
    }

    pub(crate) fn set_codex_chat_error(&mut self, message: String) {
        self.finish_codex_activity("The request could not be completed.");
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
        self.codex_chat_error = Some(message);
    }

    pub(crate) fn set_codex_chat_cancelling(&mut self) {
        self.codex_cancel_requested = true;
        self.set_codex_activity(CodexActivity::Cancelling);
        self.notice = "Stopping Codex…".to_owned();
    }

    pub(crate) fn set_codex_chat_cancelled(&mut self) {
        self.finish_codex_activity("Request cancelled. The preview was not rebuilt.");
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
    }

    pub(crate) fn finish_codex_activity(&mut self, message: &str) {
        let was_active = self.codex_activity.is_active();
        self.codex_activity = CodexActivity::Idle;
        self.codex_live_excerpt.clear();
        self.codex_live_pending_excerpt.clear();
        self.codex_live_excerpt_queue.clear();
        self.codex_live_last_published_at = None;
        self.codex_live_last_received_at = None;
        self.codex_live_needs_separator = false;
        self.codex_live_in_code_block = false;
        if was_active {
            self.codex_chat_messages.push(CodexChatMessage {
                role: CodexChatRole::Assistant,
                text: message.to_owned(),
            });
        }
    }

    pub(crate) fn set_codex_changes(&mut self, files: Vec<String>) {
        let source_change_count = files.iter().filter(|file| file.ends_with(".luau")).count();
        let visible_files = files
            .into_iter()
            .filter(|file| !file.ends_with(".luau"))
            .collect::<Vec<_>>();
        self.codex_source_change_count = source_change_count;
        self.codex_change_files = (!visible_files.is_empty()).then_some(visible_files);
        self.codex_change_review_open = false;
    }

    fn append_codex_live_excerpt(&mut self, text: &str) {
        let mut safe_lines = Vec::new();
        for raw_line in text.lines() {
            let has_leading_whitespace = raw_line.chars().next().is_some_and(char::is_whitespace);
            let has_trailing_whitespace = raw_line
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
            let line = raw_line.trim();
            if line.is_empty() {
                if !self.codex_live_in_code_block && raw_line.chars().any(char::is_whitespace) {
                    self.codex_live_needs_separator = true;
                }
                continue;
            }
            let fence_count = line.matches("```").count();
            if fence_count > 0 {
                if fence_count % 2 == 1 {
                    self.codex_live_in_code_block = !self.codex_live_in_code_block;
                }
                continue;
            }
            if self.codex_live_in_code_block {
                continue;
            }
            let line = line
                .replace("checking out the project", "reviewing the project")
                .replace("Checking out the project", "Reviewing the project")
                .replace("checking out", "reviewing")
                .replace("Checking out", "Reviewing");
            if !is_human_readable_codex_line(&line) {
                continue;
            }
            let needs_separator =
                has_leading_whitespace || self.codex_live_needs_separator || !safe_lines.is_empty();
            safe_lines.push((line, needs_separator));
            self.codex_live_needs_separator = has_trailing_whitespace;
        }
        let mut appended = false;
        for (line, needs_separator) in safe_lines {
            if needs_separator && !self.codex_live_pending_excerpt.is_empty() {
                self.codex_live_pending_excerpt.push(' ');
            }
            self.codex_live_pending_excerpt.push_str(&line);
            appended = true;
        }
        if appended {
            let now = Instant::now();
            self.codex_live_last_received_at = Some(now);
            self.queue_codex_live_excerpts(false);
            self.publish_codex_live_excerpt(now);
        }
    }

    fn queue_codex_live_excerpts(&mut self, flush_tail: bool) {
        while let Some(end) = codex_live_chunk_end(&self.codex_live_pending_excerpt, flush_tail) {
            let excerpt = self
                .codex_live_pending_excerpt
                .get(..end)
                .unwrap_or(&self.codex_live_pending_excerpt)
                .trim()
                .to_owned();
            self.codex_live_pending_excerpt = self
                .codex_live_pending_excerpt
                .get(end..)
                .unwrap_or_default()
                .trim_start()
                .to_owned();
            if excerpt.is_empty()
                || !is_human_readable_codex_line(&excerpt)
                || self.codex_live_excerpt == excerpt
                || self.codex_live_excerpt_queue.back() == Some(&excerpt)
            {
                continue;
            }
            self.codex_live_excerpt_queue.push_back(excerpt);
        }
    }

    fn publish_codex_live_excerpt(&mut self, now: Instant) {
        let display_time = codex_live_display_time(&self.codex_live_excerpt);
        if self
            .codex_live_last_published_at
            .is_some_and(|last| now.duration_since(last) < display_time)
        {
            return;
        }
        let Some(excerpt) = self.codex_live_excerpt_queue.pop_front() else {
            return;
        };
        self.codex_live_excerpt = excerpt;
        self.codex_live_last_published_at = Some(now);
    }

    pub(crate) fn advance_codex_live_activity(&mut self) {
        if self.codex_activity.is_active() {
            const INCOMPLETE_PHRASE_HOLD: Duration = Duration::from_millis(700);
            let now = Instant::now();
            let should_flush_tail = self.codex_live_excerpt_queue.is_empty()
                && self
                    .codex_live_last_received_at
                    .is_some_and(|last| now.duration_since(last) >= INCOMPLETE_PHRASE_HOLD);
            if should_flush_tail {
                self.queue_codex_live_excerpts(true);
            }
            self.publish_codex_live_excerpt(now);
        }
    }

    fn set_codex_activity(&mut self, activity: CodexActivity) {
        self.codex_activity = activity;
    }

    pub(crate) fn clear_codex_changes(&mut self) {
        self.codex_change_files = None;
        self.codex_source_change_count = 0;
        self.codex_change_review_open = false;
    }

    pub(crate) fn take_codex_undo_request(&mut self) -> bool {
        std::mem::take(&mut self.codex_undo_requested)
    }

    pub(crate) fn take_codex_cancel_request(&mut self) -> bool {
        if !self.codex_activity.is_active()
            || !self.codex_cancel_requested
            || self.codex_cancel_sent
        {
            return false;
        }
        self.codex_cancel_sent = true;
        true
    }

    pub(crate) fn set_chatgpt_pending(&mut self) {
        self.chatgpt_pending = true;
        self.chatgpt_available = true;
        self.chatgpt_error = None;
        self.notice = "Opening browser for ChatGPT sign-in…".to_owned();
    }

    pub(crate) fn set_chatgpt_account(&mut self, account: Option<ChatGptAccount>) {
        if self.chatgpt_pending {
            return;
        }
        self.chatgpt_available = true;
        self.chatgpt_account = account;
        self.chatgpt_error = None;
    }

    pub(crate) fn set_chatgpt_browser_opened(&mut self) {
        self.chatgpt_pending = true;
        self.notice =
            "Finish signing in with ChatGPT in your browser. Studio will continue automatically."
                .to_owned();
    }

    pub(crate) fn set_chatgpt_connected(&mut self, account: ChatGptAccount) {
        self.chatgpt_pending = false;
        self.chatgpt_available = true;
        self.chatgpt_error = None;
        self.notice = account
            .email
            .as_deref()
            .map(|email| format!("ChatGPT connected as {email}"))
            .unwrap_or_else(|| "ChatGPT connected".to_owned());
        self.chatgpt_account = Some(account);
    }

    pub(crate) fn set_chatgpt_error(&mut self, message: String) {
        self.chatgpt_pending = false;
        self.chatgpt_available = true;
        self.chatgpt_error = Some(message.clone());
        self.notice = message;
    }

    pub(crate) fn set_chatgpt_unavailable(&mut self, message: String) {
        let was_pending = self.chatgpt_pending;
        self.chatgpt_pending = false;
        self.chatgpt_available = false;
        self.chatgpt_account = None;
        self.chatgpt_error = Some(message.clone());
        if was_pending {
            self.notice = message;
        }
    }
}

pub(crate) fn codex_live_chunk_end(text: &str, flush_tail: bool) -> Option<usize> {
    const MIN_SENTENCE_CHARS: usize = 12;
    const MIN_TAIL_CHARS: usize = 24;
    const MAX_CHARS: usize = 120;
    let mut last_boundary = None;
    let mut char_count = 0;
    for (byte_index, character) in text.char_indices() {
        if character.is_whitespace() {
            last_boundary = Some(byte_index);
        }
        char_count += 1;
        if char_count >= MIN_SENTENCE_CHARS && matches!(character, '.' | '!' | '?') {
            return Some(byte_index + character.len_utf8());
        }
        if char_count == MAX_CHARS {
            return Some(last_boundary.unwrap_or_else(|| {
                text[byte_index..]
                    .char_indices()
                    .find_map(|(offset, character)| {
                        character.is_whitespace().then_some(byte_index + offset)
                    })
                    .unwrap_or(text.len())
            }));
        }
    }
    (flush_tail && char_count >= MIN_TAIL_CHARS).then_some(text.len())
}

pub(crate) fn codex_live_display_time(excerpt: &str) -> Duration {
    const MIN_DISPLAY_MILLIS: u64 = 1_500;
    const MAX_DISPLAY_MILLIS: u64 = 3_500;
    const MILLIS_PER_CHARACTER: u64 = 35;
    let millis = (excerpt.chars().count() as u64 * MILLIS_PER_CHARACTER)
        .clamp(MIN_DISPLAY_MILLIS, MAX_DISPLAY_MILLIS);
    Duration::from_millis(millis)
}

pub(crate) fn is_human_readable_codex_line(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    let lower = line.to_ascii_lowercase();
    if lower.contains(".luau")
        || lower.contains("src/")
        || lower.contains("src\\")
        || line.contains('`')
    {
        return false;
    }
    let first_word = line.split_whitespace().next().unwrap_or_default();
    if matches!(
        first_word,
        "local"
            | "function"
            | "return"
            | "if"
            | "elseif"
            | "else"
            | "for"
            | "while"
            | "repeat"
            | "until"
            | "end"
            | "require"
            | "export"
            | "import"
    ) {
        return false;
    }
    !matches!(line.chars().next(), Some('-' | '{' | '}' | '(' | ')'))
        && !line.contains(" = ")
        && !line.contains("=>")
        && !(line.contains('(') && line.contains(')'))
}
