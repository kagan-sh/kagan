use std::path::PathBuf;

use gpui_kit::AsyncApp;
use kg_core::{Requester, Workspace};
use kg_domain::{InteractionOption, OpError, ThreadId};
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
    choose(thread_id, requester, title, &question, options(verb), cx).await
}

/// Cancel, and the one thing a person can agree to.
pub(in crate::runtime::ops) fn options(verb: &str) -> Vec<InteractionOption> {
    vec![
        InteractionOption {
            id: InteractionOption::DECLINE.into(),
            label: "Cancel".into(),
        },
        InteractionOption {
            id: InteractionOption::APPROVE.into(),
            label: verb.into(),
        },
    ]
}

pub(in crate::runtime::ops) async fn choose(
    thread_id: Option<&ThreadId>,
    requester: Option<Requester>,
    title: String,
    question: &str,
    options: Vec<InteractionOption>,
    cx: &mut AsyncApp,
) -> Result<()> {
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
