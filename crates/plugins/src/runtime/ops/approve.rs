use std::path::PathBuf;

use gpui_kit::AsyncApp;
use kg_core::{Requester, Workspace};
use kg_domain::{InteractionOption, OpError, OptionRole, ThreadId};
use serde_json::Value;

use crate::Result;
use crate::manifest::Setting;

/// `about` says where the code comes from, between the question and the powers.
pub(in crate::runtime::ops) async fn approve(
    thread_id: Option<&ThreadId>,
    requester: Option<Requester>,
    question: String,
    about: Vec<String>,
    lines: Vec<String>,
    verb: &str,
    cx: &mut AsyncApp,
) -> Result<()> {
    let head = std::iter::once(question.clone())
        .chain(about)
        .collect::<Vec<_>>()
        .join("\n");
    let title = if lines.is_empty() {
        format!(
            "{head}\nIt asks for no extra powers. It cannot run programs or read your other files. It can read its own files and store data."
        )
    } else {
        let lines: Vec<String> = lines.iter().map(|line| format!("- {line}")).collect();
        format!("{head}\nIt:\n{}", lines.join("\n"))
    };
    choose(
        thread_id,
        requester,
        title,
        &question,
        options(verb, false),
        cx,
    )
    .await
}

/// Cancel, and the one thing a person can agree to.
pub(in crate::runtime::ops) fn options(verb: &str, destructive: bool) -> Vec<InteractionOption> {
    vec![
        InteractionOption {
            id: InteractionOption::DECLINE.into(),
            label: "Cancel".into(),
            role: Some(OptionRole::Safe),
        },
        InteractionOption {
            id: InteractionOption::APPROVE.into(),
            label: verb.into(),
            role: destructive.then_some(OptionRole::Destructive),
        },
    ]
}

// A preview has no one to answer its questions, so a question there is refused, not left to wait.
pub(in crate::runtime::ops) const NO_ONE_HERE: &str = "this is a preview kagan; no one can answer here. Run this without KG_DATA_DIR and KG_SOCKET set";

pub(in crate::runtime::ops) fn refuse_in_preview(cx: &mut AsyncApp) -> Result<()> {
    if cx.update(|cx| Workspace::global(cx).read(cx).is_preview()) {
        return Err(OpError::Invalid(NO_ONE_HERE.into()));
    }
    Ok(())
}

pub(in crate::runtime::ops) async fn choose(
    thread_id: Option<&ThreadId>,
    requester: Option<Requester>,
    title: String,
    question: &str,
    options: Vec<InteractionOption>,
    cx: &mut AsyncApp,
) -> Result<()> {
    refuse_in_preview(cx)?;
    let answer = cx
        .update(|cx| Workspace::ask_for_person(thread_id, requester, title, options, cx))
        .await?;
    if answer == InteractionOption::APPROVE {
        Ok(())
    } else {
        Err(OpError::Invalid(format!("declined: {question}")))
    }
}

pub(in crate::runtime::ops) async fn approve_folder(
    setting: &Setting,
    value: &Value,
    question: String,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<Option<PathBuf>> {
    let (Setting::Folder { title }, Some(path)) = (setting, value.as_str()) else {
        return Ok(None);
    };
    let reads = vec![format!("reads {path} ({title})")];
    approve(
        thread_id.as_ref(),
        requester,
        question,
        Vec::new(),
        reads,
        "Approve",
        cx,
    )
    .await?;
    Ok(Some(PathBuf::from(path)))
}
