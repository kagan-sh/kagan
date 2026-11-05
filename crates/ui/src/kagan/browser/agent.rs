//! The page a thread drives: one per window, one owner, and the snapshot it reads.

use std::time::{Duration, Instant};

use gpui_kit::{AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, Window};
use kg_domain::{BrowserOp, BrowserView, OpError, OpResult, ThreadId};

use super::Agent;
use super::snapshot::{INFO, SNAPSHOT, UNREADABLE, parse_info, parse_snapshot};
use crate::kagan::{Kagan, tabs};
use crate::web_page::WebPage;

const NEEDS_WINDOW: &str = "browser control needs a kagan window";
const NO_SESSION: &str = "no browser session; run kg browser open <url> first";
const ENDED: &str = "the browser session ended";
const IDLE_ENDED: &str = "the browser session ended after 30 idle minutes";
const DEADLINE: &str = "the page did not answer within 10 seconds";
const BAD_URL: &str = "type a web address, such as example.com";
/// How long a drive may sit unused before its next command ends it.
const IDLE: Duration = Duration::from_mins(30);
/// How long one command waits for the page.
const WAIT: Duration = Duration::from_secs(10);

/// Runs one Browser verb for the window's page.
///
/// # Errors
///
/// When no window is open, when another thread owns the page, or when the page cannot be read.
pub(crate) async fn run(op: BrowserOp, cx: &mut AsyncApp) -> Result<OpResult, OpError> {
    let Some((handle, kagan)) = cx.update(|cx| crate::kagan::front(cx)) else {
        return Err(OpError::Failed(NEEDS_WINDOW.into()));
    };
    match op {
        BrowserOp::Open { url, thread_id } => open(&kagan, handle, &thread_id, &url, cx).await,
        BrowserOp::Status { thread_id } => status(&kagan, handle, &thread_id, cx).await,
        BrowserOp::Snapshot { thread_id } => snapshot(&kagan, handle, &thread_id, cx).await,
        BrowserOp::Goto { url, thread_id } => goto(&kagan, handle, &thread_id, &url, cx).await,
        BrowserOp::Back { thread_id } => history(&kagan, handle, &thread_id, true, cx),
        BrowserOp::Forward { thread_id } => history(&kagan, handle, &thread_id, false, cx),
        BrowserOp::Reload { thread_id } => reload(&kagan, handle, &thread_id, cx).await,
        BrowserOp::Close { thread_id } => close(&kagan, handle, &thread_id, cx),
    }
}

async fn open(
    kagan: &Entity<Kagan>,
    handle: AnyWindowHandle,
    thread_id: &ThreadId,
    typed: &str,
    cx: &mut AsyncApp,
) -> Result<OpResult, OpError> {
    let Some(url) = super::page_url(typed) else {
        return Err(OpError::Invalid(BAD_URL.into()));
    };
    let page = on_window(kagan, handle, cx, |kagan, window, cx| {
        kagan.begin_open(thread_id, &url, window, cx)
    })??;
    wait_for_load(kagan, handle, &page, cx).await;
    let info = evaluate(&page, INFO, handle, WAIT, cx)
        .await
        .ok()
        .and_then(|text| parse_info(&text).ok())
        .map(|(url, title, _)| (url, title));
    let view = on_window(kagan, handle, cx, |kagan, _, cx| {
        kagan.view(thread_id, info, cx)
    })??;
    Ok(OpResult::Browser(view))
}

async fn status(
    kagan: &Entity<Kagan>,
    handle: AnyWindowHandle,
    thread_id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult, OpError> {
    let page = on_window(kagan, handle, cx, |kagan, window, cx| {
        kagan.require(thread_id, window, cx)
    })??;
    let text = evaluate(&page, INFO, handle, WAIT, cx).await?;
    let (url, title, _) = parse_info(&text)?;
    let view = on_window(kagan, handle, cx, |kagan, _, cx| {
        kagan.view(thread_id, Some((url, title)), cx)
    })??;
    Ok(OpResult::Browser(view))
}

async fn snapshot(
    kagan: &Entity<Kagan>,
    handle: AnyWindowHandle,
    thread_id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult, OpError> {
    let page = on_window(kagan, handle, cx, |kagan, window, cx| {
        kagan.require(thread_id, window, cx)
    })??;
    let text = evaluate(&page, SNAPSHOT, handle, WAIT, cx).await?;
    let snapshot = parse_snapshot(&text)?;
    on_window(kagan, handle, cx, |kagan, _, cx| {
        kagan.touch(&page, cx)?;
        Ok::<(), OpError>(())
    })??;
    Ok(OpResult::BrowserSnapshot(snapshot))
}

async fn goto(
    kagan: &Entity<Kagan>,
    handle: AnyWindowHandle,
    thread_id: &ThreadId,
    typed: &str,
    cx: &mut AsyncApp,
) -> Result<OpResult, OpError> {
    let Some(url) = super::page_url(typed) else {
        return Err(OpError::Invalid(BAD_URL.into()));
    };
    let page = on_window(kagan, handle, cx, |kagan, window, cx| {
        kagan.require(thread_id, window, cx)
    })??;
    let url = url.as_str().to_owned();
    on_window(kagan, handle, cx, |_, _, cx| {
        page.read(cx).forget_loads();
        page.update(cx, |page, cx| page.load_url(&url, cx));
    })?;
    wait_for_load(kagan, handle, &page, cx).await;
    touch(kagan, handle, &page, cx)?;
    Ok(OpResult::Done)
}

fn history(
    kagan: &Entity<Kagan>,
    handle: AnyWindowHandle,
    thread_id: &ThreadId,
    back: bool,
    cx: &mut AsyncApp,
) -> Result<OpResult, OpError> {
    let page = on_window(kagan, handle, cx, |kagan, window, cx| {
        kagan.require(thread_id, window, cx)
    })??;
    on_window(kagan, handle, cx, |_, _, cx| {
        let moved = if back {
            page.update(cx, |page, cx| page.back(cx))
        } else {
            page.update(cx, |page, cx| page.forward(cx))
        };
        moved.map_err(OpError::Failed)
    })??;
    touch(kagan, handle, &page, cx)?;
    Ok(OpResult::Done)
}

async fn reload(
    kagan: &Entity<Kagan>,
    handle: AnyWindowHandle,
    thread_id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult, OpError> {
    let page = on_window(kagan, handle, cx, |kagan, window, cx| {
        kagan.require(thread_id, window, cx)
    })??;
    on_window(kagan, handle, cx, |_, _, cx| {
        page.read(cx).forget_loads();
        page.read(cx).reload(cx).map_err(OpError::Failed)
    })??;
    wait_for_load(kagan, handle, &page, cx).await;
    touch(kagan, handle, &page, cx)?;
    Ok(OpResult::Done)
}

fn close(
    kagan: &Entity<Kagan>,
    handle: AnyWindowHandle,
    thread_id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult, OpError> {
    on_window(kagan, handle, cx, |kagan, window, cx| {
        kagan.require(thread_id, window, cx)?;
        kagan.end_agent(window, cx);
        kagan.sync_browser(cx);
        cx.notify();
        Ok::<(), OpError>(())
    })??;
    Ok(OpResult::Done)
}

fn on_window<R>(
    kagan: &Entity<Kagan>,
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
    job: impl FnOnce(&mut Kagan, &mut Window, &mut Context<Kagan>) -> R,
) -> Result<R, OpError> {
    cx.update_window(handle, |_, window, cx| {
        kagan.update(cx, |kagan, cx| job(kagan, window, cx))
    })
    .map_err(|_| OpError::NotFound(ENDED.into()))
}

/// Runs one host script in the page and waits for its JSON reply.
async fn evaluate(
    page: &Entity<WebPage>,
    script: &'static str,
    handle: AnyWindowHandle,
    wait: Duration,
    cx: &mut AsyncApp,
) -> Result<String, OpError> {
    let (sender, receiver) = async_channel::bounded::<Option<String>>(1);
    let reply = sender.clone();
    cx.update_window(handle, |_, _, cx| {
        page.read(cx)
            .eval(script, reply, cx)
            .map_err(OpError::Failed)
    })
    .map_err(|_| OpError::NotFound(ENDED.into()))??;
    cx.spawn(async move |cx| {
        cx.background_executor().timer(wait).await;
        let _ = sender.try_send(None);
    })
    .detach();
    match receiver.recv().await {
        Ok(Some(text)) if !text.trim().is_empty() => Ok(text),
        Ok(Some(_)) | Err(_) => Err(OpError::Failed(UNREADABLE.into())),
        Ok(None) => Err(OpError::Failed(DEADLINE.into())),
    }
}

/// Waits for the document that `load_url` started, or for the deadline.
async fn wait_for_load(
    kagan: &Entity<Kagan>,
    handle: AnyWindowHandle,
    page: &Entity<WebPage>,
    cx: &mut AsyncApp,
) {
    let Ok((signal, wait)) = on_window(kagan, handle, cx, |_, _, cx| page.read(cx).loading())
    else {
        return;
    };
    cx.spawn(async move |cx| {
        cx.background_executor().timer(WAIT).await;
        let _ = signal.try_send(());
    })
    .detach();
    let _ = wait.recv().await;
}

fn touch(
    kagan: &Entity<Kagan>,
    handle: AnyWindowHandle,
    page: &Entity<WebPage>,
    cx: &mut AsyncApp,
) -> Result<(), OpError> {
    on_window(kagan, handle, cx, |kagan, _, cx| kagan.touch(page, cx))?
}

impl Kagan {
    /// The page this thread drives, after the idle rule and the ownership rule.
    fn require(
        &mut self,
        thread_id: &ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Entity<WebPage>, OpError> {
        if self
            .browser
            .agent
            .as_ref()
            .is_some_and(|agent| agent.thread == *thread_id && idle(agent.used, Instant::now()))
        {
            self.end_agent(window, cx);
            self.sync_browser(cx);
            cx.notify();
            return Err(OpError::NotFound(IDLE_ENDED.into()));
        }
        let Some(agent) = &self.browser.agent else {
            return Err(OpError::NotFound(NO_SESSION.into()));
        };
        if !owns(&agent.thread, thread_id) {
            return Err(OpError::Invalid(self.owner_refusal(&agent.thread, cx)));
        }
        Ok(agent.page.clone())
    }

    /// Gives the page to this thread, or loads the URL in the page it already drives.
    fn begin_open(
        &mut self,
        thread_id: &ThreadId,
        url: &url::Url,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Entity<WebPage>, OpError> {
        if let Some(agent) = &self.browser.agent
            && !owns(&agent.thread, thread_id)
        {
            return Err(OpError::Invalid(self.owner_refusal(&agent.thread, cx)));
        }
        if self
            .browser
            .agent
            .as_ref()
            .is_some_and(|agent| idle(agent.used, Instant::now()))
        {
            self.end_agent(window, cx);
        }
        let page = if let Some(agent) = &mut self.browser.agent {
            let page = agent.page.clone();
            page.read(cx).forget_loads();
            page.update(cx, |page, cx| page.load_url(url.as_str(), cx));
            agent.used = Instant::now();
            url.as_str().clone_into(&mut agent.url);
            page
        } else {
            // One tab, one page: a person's page gives way, and a fresh document drops old refs.
            self.browser.page = None;
            let page = self
                .browser
                .page_for(url, window, cx)
                .map_err(OpError::Failed)?;
            self.browser.agent = Some(Agent {
                thread: thread_id.clone(),
                page: page.clone(),
                url: url.as_str().to_owned(),
                used: Instant::now(),
            });
            page
        };
        if self.shown_thread(cx).as_ref() == Some(thread_id) {
            self.tabs.browser = true;
            self.select_right_tab(tabs::BROWSER, cx);
            self.show_right_quiet(window, cx);
        }
        self.sync_browser(cx);
        cx.notify();
        Ok(page)
    }

    /// The page as `open` and `status` report it.
    fn view(
        &mut self,
        thread_id: &ThreadId,
        info: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) -> Result<BrowserView, OpError> {
        let Some(agent) = &self.browser.agent else {
            return Err(OpError::NotFound(ENDED.into()));
        };
        let url = info
            .as_ref()
            .map_or_else(|| agent.page.read(cx).url(cx), |(url, _)| url.clone());
        let title = info.map_or_else(String::new, |(_, title)| title);
        let thread = self
            .record(thread_id, cx)
            .map_or_else(|| thread_id.to_string(), |thread| thread.title.clone());
        let shown = self.browser_drawn(cx);
        if let Some(agent) = &mut self.browser.agent {
            agent.url.clone_from(&url);
            agent.used = Instant::now();
        }
        Ok(BrowserView {
            thread_id: thread_id.clone(),
            thread,
            url,
            title,
            shown,
        })
    }

    /// Records that the page answered, and where it is now.
    fn touch(&mut self, page: &Entity<WebPage>, cx: &App) -> Result<(), OpError> {
        let Some(agent) = &mut self.browser.agent else {
            return Err(OpError::NotFound(ENDED.into()));
        };
        agent.url = page.read(cx).url(cx);
        agent.used = Instant::now();
        Ok(())
    }

    /// Ends control and leaves the page as the person's, with its address in the field.
    pub(super) fn end_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = self.browser.agent.take() else {
            return;
        };
        self.browser.page = Some(agent.page);
        self.browser.address.update(cx, |address, cx| {
            address.set_value(agent.url, window, cx);
        });
    }

    fn owner_refusal(&self, owner: &ThreadId, cx: &App) -> String {
        let owner = self
            .record(owner, cx)
            .map_or_else(|| owner.to_string(), |thread| thread.title.clone());
        format!("thread {owner} is driving the browser; close it with kg browser close")
    }

    /// Shows the right sidebar without moving the keyboard, which belongs to the person.
    fn show_right_quiet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.right_drawn() {
            return;
        }
        if self.screen.review().is_none() && self.room_panes(self.layout.left_open, true).right {
            self.force_sidebar(false, true, window, cx);
        } else {
            self.open_overlay(
                crate::kagan::screen::Side::Right,
                crate::kagan::screen::Activation::Quiet,
                window,
                cx,
            );
        }
    }
}

/// Whether `caller` may act on the page `owner` drives.
fn owns(owner: &ThreadId, caller: &ThreadId) -> bool {
    owner == caller
}

fn idle(used: Instant, now: Instant) -> bool {
    now.saturating_duration_since(used) > IDLE
}

#[cfg(test)]
mod tests {
    use super::{idle, owns};
    use kg_domain::ThreadId;
    use std::time::{Duration, Instant};

    fn thread(name: &str) -> ThreadId {
        ThreadId(name.to_owned())
    }

    #[test]
    fn only_the_owner_thread_may_drive_the_page() {
        assert!(owns(&thread("a"), &thread("a")));
        assert!(!owns(&thread("a"), &thread("b")));
    }

    #[test]
    fn thirty_idle_minutes_end_the_drive() {
        let used = Instant::now();
        assert!(!idle(used, used + Duration::from_mins(29)));
        assert!(idle(used, used + Duration::from_mins(31)));
    }
}
