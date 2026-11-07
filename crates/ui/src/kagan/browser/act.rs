//! The fixed scripts that act on the page, and the text and key they carry.

#[cfg(any(test, desktop))]
use kg_domain::BrowserOp;

/// A file input is refused before any event, because it would open the person's chooser.
pub(super) const FILE_REFUSAL: &str = "a page cannot choose a file; the person uploads it";

const KEYS: &str = "press enter, escape, tab, space, backspace, delete, up, down, left, right, home, end, pageup, pagedown, or one printable character";

const HEAD: &str = r#"(() => {
  try {
    const store = window[Symbol.for("kagan.browser")];
    const refs = store && store.refs ? store.refs : [];
    const missing = (ref) => ({ ok: false, code: "not-found", message: "ref " + ref + " is not on the page; take a fresh snapshot" });
    const refuse = (message) => ({ ok: false, code: "invalid", message: message });
    const ok = () => ({ ok: true, url: location.href, title: document.title || "" });
    const send = (element, type, options) => element.dispatchEvent(new (window.PointerEvent || MouseEvent)(type, options));
    const change = (element) => {
      element.dispatchEvent(new Event("input", { bubbles: true }));
      element.dispatchEvent(new Event("change", { bubbles: true }));
    };
    const put = (element, value) => {
      const tag = element.tagName.toLowerCase();
      const proto = tag === "textarea" ? HTMLTextAreaElement.prototype : tag === "input" ? HTMLInputElement.prototype : null;
      const setter = proto && Object.getOwnPropertyDescriptor(proto, "value");
      if (setter && setter.set) setter.set.call(element, value); else element.value = value;
    };
"#;

const TAIL: &str = r#"
  } catch (error) {
    return { ok: false, code: "failed", message: String(error && error.message || error) };
  }
})()"#;

/// A text action never relays the page's exception, which may quote the value it was given.
const TEXT_TAIL: &str = r#"
  } catch (error) {
    return { ok: false, code: "failed", message: "the page refused the text" };
  }
})()"#;

fn finish(body: &str) -> String {
    format!("{HEAD}{body}{TAIL}")
}

fn finish_text(body: &str) -> String {
    format!("{HEAD}{body}{TEXT_TAIL}")
}

/// Embeds the text in the script as a JSON string, so a quote or newline cannot end it.
fn quoted(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

#[cfg(any(test, desktop))]
pub(super) fn script(op: &BrowserOp) -> Result<String, String> {
    Ok(match op {
        BrowserOp::Click { ref_id, .. } => click(*ref_id),
        BrowserOp::Fill { ref_id, text, .. } => fill(*ref_id, text.as_deref().unwrap_or_default()),
        BrowserOp::Type { text, .. } => type_text(text),
        BrowserOp::Press { key, .. } => press(key)?,
        BrowserOp::Select { ref_id, option, .. } => select(*ref_id, option),
        BrowserOp::Scroll { dy, .. } => scroll(*dy),
        _ => return Err("this browser verb does not act on the page".into()),
    })
}

pub(super) fn click(ref_id: u32) -> String {
    let body = r#"
    const ref = __REF__;
    const element = refs[ref - 1];
    if (!element || !element.isConnected) return missing(ref);
    if (element.tagName === "INPUT" && (element.getAttribute("type") || "").toLowerCase() === "file") return refuse(__FILE__);
    element.focus({ preventScroll: true });
    const options = { bubbles: true, cancelable: true, composed: true };
    send(element, "pointerdown", options);
    element.dispatchEvent(new MouseEvent("mousedown", options));
    send(element, "pointerup", options);
    element.dispatchEvent(new MouseEvent("mouseup", options));
    element.dispatchEvent(new MouseEvent("click", options));
    return ok();
"#
    .replace("__REF__", &ref_id.to_string())
    .replace("__FILE__", &quoted(FILE_REFUSAL));
    finish(&body)
}

pub(super) fn fill(ref_id: u32, text: &str) -> String {
    let body = r#"
    const ref = __REF__;
    const element = refs[ref - 1];
    if (!element || !element.isConnected) return missing(ref);
    if (element.tagName === "INPUT" && (element.getAttribute("type") || "").toLowerCase() === "file") return refuse(__FILE__);
    element.focus({ preventScroll: true });
    if (element.isContentEditable) {
      document.execCommand("insertText", false, __TEXT__);
    } else {
      put(element, __TEXT__);
      change(element);
    }
    return ok();
"#
    .replace("__REF__", &ref_id.to_string())
    .replace("__FILE__", &quoted(FILE_REFUSAL))
    .replace("__TEXT__", &quoted(text));
    finish_text(&body)
}

pub(super) fn type_text(text: &str) -> String {
    let body = r#"
    const element = document.activeElement;
    if (!element) return ok();
    const text = __TEXT__;
    if (element.isContentEditable || element.tagName === "INPUT" || element.tagName === "TEXTAREA") {
      element.focus({ preventScroll: true });
      if (!document.execCommand("insertText", false, text) && (element.tagName === "INPUT" || element.tagName === "TEXTAREA")) {
        put(element, String(element.value == null ? "" : element.value) + text);
        change(element);
      }
    }
    return ok();
"#
    .replace("__TEXT__", &quoted(text));
    finish_text(&body)
}

pub(super) fn press(key: &str) -> Result<String, String> {
    let typed = if key == " " { key } else { key.trim() };
    let lower = typed.to_ascii_lowercase();
    let (key, code, key_code) = match lower.as_str() {
        "enter" => ("Enter".to_owned(), "Enter".to_owned(), 13),
        "escape" => ("Escape".to_owned(), "Escape".to_owned(), 27),
        "tab" => ("Tab".to_owned(), "Tab".to_owned(), 9),
        "space" => (" ".to_owned(), "Space".to_owned(), 32),
        "backspace" => ("Backspace".to_owned(), "Backspace".to_owned(), 8),
        "delete" => ("Delete".to_owned(), "Delete".to_owned(), 46),
        "up" | "arrowup" => ("ArrowUp".to_owned(), "ArrowUp".to_owned(), 38),
        "down" | "arrowdown" => ("ArrowDown".to_owned(), "ArrowDown".to_owned(), 40),
        "left" | "arrowleft" => ("ArrowLeft".to_owned(), "ArrowLeft".to_owned(), 37),
        "right" | "arrowright" => ("ArrowRight".to_owned(), "ArrowRight".to_owned(), 39),
        "home" => ("Home".to_owned(), "Home".to_owned(), 36),
        "end" => ("End".to_owned(), "End".to_owned(), 35),
        "pageup" => ("PageUp".to_owned(), "PageUp".to_owned(), 33),
        "pagedown" => ("PageDown".to_owned(), "PageDown".to_owned(), 34),
        _ => {
            let mut chars = typed.chars();
            let (Some(ch), None) = (chars.next(), chars.next()) else {
                return Err(format!("unknown key {typed}; {KEYS}"));
            };
            if ch.is_control() {
                return Err(format!("unknown key {typed}; {KEYS}"));
            }
            let code = if ch.is_ascii_alphabetic() {
                format!("Key{}", ch.to_ascii_uppercase())
            } else if ch.is_ascii_digit() {
                format!("Digit{ch}")
            } else {
                String::new()
            };
            let key_code = ch.to_uppercase().next().map_or(0, |upper| upper as u32);
            (ch.to_string(), code, key_code)
        }
    };
    let body = r#"
    const element = document.activeElement || document.body;
    if (!element) return ok();
    const options = { bubbles: true, cancelable: true, composed: true, key: __KEY__, code: __CODE__, keyCode: __KEYCODE__, which: __KEYCODE__ };
    const down = new KeyboardEvent("keydown", options);
    element.dispatchEvent(down);
    const pressed = new KeyboardEvent("keypress", options);
    element.dispatchEvent(pressed);
    element.dispatchEvent(new KeyboardEvent("keyup", options));
    if (!down.defaultPrevented && !pressed.defaultPrevented && __KEY__ === "Tab") {
      const focusable = Array.from(document.querySelectorAll("a[href],button,input:not([type=hidden]),select,textarea,[tabindex]:not([tabindex='-1']),[contenteditable]"))
        .filter((item) => item.getClientRects().length > 0 && !item.disabled);
      const next = focusable[(focusable.indexOf(element) + 1) % focusable.length];
      if (next) next.focus();
    }
    if (!down.defaultPrevented && !pressed.defaultPrevented && __KEY__ === "Enter") {
      const tag = element.tagName;
      const form = element.form || element.closest("form");
      if (tag === "BUTTON" || (tag === "INPUT" && /^(submit|button)$/i.test(element.type || ""))) {
        element.click();
      } else if (tag === "INPUT" && form) {
        const submit = form.querySelector("button[type=submit],input[type=submit],button:not([type])");
        // A submit button runs validation and the submit event; `requestSubmit` does the same;
        // `submit` skips both, and is the fallback for macOS 11 and 12, whose WebKit predates it.
        if (submit) submit.click();
        else if (form.requestSubmit) form.requestSubmit();
        else form.submit();
      }
    }
    return ok();
"#
    .replace("__KEY__", &quoted(&key))
    .replace("__CODE__", &quoted(&code))
    .replace("__KEYCODE__", &key_code.to_string());
    Ok(finish(&body))
}

pub(super) fn select(ref_id: u32, option: &str) -> String {
    let body = r#"
    const ref = __REF__;
    const element = refs[ref - 1];
    if (!element || !element.isConnected) return missing(ref);
    if (element.tagName !== "SELECT") return refuse("ref " + ref + " is not a select");
    const options = Array.from(element.options);
    const option = options.find((item) => item.value === __OPTION__ || (item.label || item.text) === __OPTION__);
    if (!option) {
      const labels = options.slice(0, 10).map((item) => item.label || item.text);
      return refuse("no option " + __OPTION__ + "; choose one of: " + labels.join(", "));
    }
    element.value = option.value;
    change(element);
    return ok();
"#
    .replace("__REF__", &ref_id.to_string())
    .replace("__OPTION__", &quoted(option));
    finish(&body)
}

pub(super) fn scroll(dy: f32) -> String {
    let dy = if dy.is_finite() {
        dy.to_string()
    } else {
        "0".to_owned()
    };
    let body = r#"
    window.scrollBy({ top: __DY__, left: 0, behavior: "instant" });
    return ok();
"#
    .replace("__DY__", &dy);
    finish(&body)
}

#[cfg(test)]
mod tests {
    use super::{FILE_REFUSAL, click, fill, press, script, scroll, select, type_text};
    use crate::kagan::browser::snapshot::parse_info;
    use kg_domain::{BrowserOp, OpError, ThreadId};

    #[test]
    fn a_text_argument_stays_inside_a_json_string() {
        let text = "say \"hi\"\n</script>`date`";
        let encoded = serde_json::to_string(text).expect("a JSON string");
        for script in [fill(2, text), type_text(text), select(2, text)] {
            assert!(script.contains(&encoded), "{script}");
            assert!(!script.contains("say \"hi\""), "a quote stays encoded");
            assert!(
                !script.contains("hi\"\n</script>"),
                "a newline stays encoded"
            );
        }
    }

    #[test]
    fn a_dead_ref_asks_for_a_fresh_snapshot() {
        for script in [click(7), fill(7, "x"), select(7, "x")] {
            assert!(
                script.contains("take a fresh snapshot"),
                "a dead ref says how to recover: {script}"
            );
        }
        assert_eq!(
            parse_info(
                r#"{"ok":false,"code":"not-found","message":"ref 7 is not on the page; take a fresh snapshot"}"#
            ),
            Err(OpError::NotFound(
                "ref 7 is not on the page; take a fresh snapshot".into()
            ))
        );
    }

    #[test]
    fn a_file_input_is_refused_before_any_event() {
        for script in [click(1), fill(1, "x")] {
            let refusal = script.find(FILE_REFUSAL).expect("the refusal");
            let event = script.find("pointerdown").or_else(|| script.find("put("));
            assert!(refusal < event.expect("an event or a write"), "{script}");
        }
    }

    #[test]
    fn a_wrong_option_lists_up_to_ten_labels() {
        let script = select(2, "nope");
        assert!(script.contains("slice(0, 10)"), "{script}");
        assert!(script.contains("choose one of: "), "{script}");
        assert_eq!(
            parse_info(
                r#"{"ok":false,"code":"invalid","message":"no option \"nope\"; choose one of: Red, Blue"}"#
            ),
            Err(OpError::Invalid(
                "no option \"nope\"; choose one of: Red, Blue".into()
            ))
        );
    }

    #[test]
    fn press_carries_the_key_code_and_key_code() {
        let enter = press("enter").expect("enter");
        assert!(enter.contains(r#"key: "Enter""#));
        assert!(enter.contains(r#"code: "Enter""#));
        assert!(enter.contains("keyCode: 13"));
        let up = press("arrowup").expect("an arrow");
        assert!(up.contains(r#"key: "ArrowUp""#));
        assert!(up.contains("keyCode: 38"));
        let letter = press("a").expect("a letter");
        assert!(letter.contains(r#"key: "a""#));
        assert!(letter.contains(r#"code: "KeyA""#));
        assert!(letter.contains("keyCode: 65"));
        let space = press("space").expect("space");
        assert!(space.contains(r#"key: " ""#));
        assert!(space.contains(r#"code: "Space""#));
        assert!(space.contains("keyCode: 32"));
        assert!(press("f13").is_err());
        assert!(press("").is_err());
        assert!(press(" ").is_ok());
        assert!(press("\t").is_err());
    }

    #[test]
    fn scroll_embeds_a_finite_pixel_count() {
        for (dy, want) in [
            (-240.0, "top: -240,"),
            (0.0, "top: 0,"),
            (10_000.0, "top: 10000,"),
        ] {
            let script = scroll(dy);
            assert!(script.contains(want), "{script}");
            assert!(!script.contains("__DY__"), "{script}");
        }
        assert!(scroll(f32::INFINITY).contains("top: 0,"));
    }

    #[test]
    fn script_maps_the_acting_verbs_and_refuses_the_rest() {
        let thread = || ThreadId("thr_1".into());
        let built = |op: BrowserOp| script(&op).expect("a script");
        assert!(
            built(BrowserOp::Click {
                ref_id: 1,
                thread_id: thread()
            })
            .contains("pointerdown")
        );
        assert!(
            built(BrowserOp::Fill {
                ref_id: 1,
                text: Some("x".into()),
                file: None,
                thread_id: thread(),
            })
            .contains("put(element,")
        );
        assert!(
            built(BrowserOp::Type {
                text: "x".into(),
                thread_id: thread()
            })
            .contains("insertText")
        );
        assert!(
            built(BrowserOp::Press {
                key: "enter".into(),
                thread_id: thread()
            })
            .contains(r#"key: "Enter""#)
        );
        assert!(
            built(BrowserOp::Select {
                ref_id: 1,
                option: "x".into(),
                thread_id: thread(),
            })
            .contains("options.find")
        );
        assert!(
            built(BrowserOp::Scroll {
                dy: 5.0,
                thread_id: thread()
            })
            .contains("scrollBy")
        );
        assert_eq!(
            script(&BrowserOp::Open {
                url: "x".into(),
                thread_id: thread()
            }),
            Err("this browser verb does not act on the page".into())
        );
        assert!(
            script(&BrowserOp::Press {
                key: "f13".into(),
                thread_id: thread()
            })
            .is_err()
        );
    }

    #[test]
    fn a_text_action_never_relays_the_page_exception() {
        for script in [fill(1, "hunter2"), type_text("hunter2")] {
            assert!(script.contains("the page refused the text"), "{script}");
            assert!(!script.contains("error && error.message"), "{script}");
        }
    }
}
