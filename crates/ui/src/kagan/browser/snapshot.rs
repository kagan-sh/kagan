//! The scripts that read the page, and the parser that turns their reply into a snapshot.

use kg_domain::{BrowserControl, BrowserSnapshot, OpError};
use serde::Deserialize;

pub(super) const UNREADABLE: &str = "the page could not be read; take the page over or try again";

#[derive(Deserialize)]
struct Wire {
    ok: bool,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    ready: bool,
    #[serde(default)]
    controls: Vec<WireControl>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct WireControl {
    #[serde(rename = "ref")]
    ref_id: u32,
    role: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    checked: Option<bool>,
}

fn wire(raw: &str) -> Result<Wire, OpError> {
    if raw.trim().is_empty() {
        return Err(OpError::Failed(UNREADABLE.into()));
    }
    serde_json::from_str(raw).map_err(|_| OpError::Failed(UNREADABLE.into()))
}

fn answered(raw: &str) -> Result<Wire, OpError> {
    let wire = wire(raw)?;
    if wire.ok {
        return Ok(wire);
    }
    let message = wire.message.unwrap_or_else(|| UNREADABLE.into());
    Err(match wire.code.as_deref() {
        Some("not-found") => OpError::NotFound(message),
        Some("invalid") => OpError::Invalid(message),
        _ => OpError::Failed(message),
    })
}

pub(super) fn parse_info(raw: &str) -> Result<(String, String, bool), OpError> {
    let wire = answered(raw)?;
    Ok((wire.url, wire.title, wire.ready))
}

pub(super) fn parse_snapshot(raw: &str) -> Result<BrowserSnapshot, OpError> {
    let wire = answered(raw)?;
    let controls = wire
        .controls
        .into_iter()
        .map(|control| {
            // The script never reads a password; this is the second guard on the way to a person.
            let password = control.role == "password";
            BrowserControl {
                ref_id: control.ref_id,
                role: control.role,
                name: control.name,
                value: control.value.filter(|_| !password),
                focused: control.focused,
                disabled: control.disabled,
                checked: control.checked,
            }
        })
        .collect();
    Ok(BrowserSnapshot {
        url: wire.url,
        title: wire.title,
        controls,
        text: wire.text,
        truncated: wire.truncated,
    })
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) const INFO: &str = r#"(() => {
  try {
    return { ok: true, url: location.href, title: document.title || "", ready: document.readyState === "complete" };
  } catch (error) {
    return { ok: false, code: "failed", message: String(error && error.message || error) };
  }
})()"#;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) const SNAPSHOT: &str = r#"(() => {
  const LIMIT = { controls: 300, name: 120, value: 200, text: 8192, json: 24576 };
  const clip = (text, limit) => {
    const flat = String(text == null ? "" : text).replace(/\s+/g, " ").trim();
    return flat.length > limit ? flat.slice(0, limit) + "\u2026" : flat;
  };
  try {
    const store = window[Symbol.for("kagan.browser")] || (window[Symbol.for("kagan.browser")] = { refs: [] });
    const refs = [];
    store.refs = refs;
    const nameOf = (element) => {
      const aria = (element.getAttribute("aria-label") || "").trim();
      if (aria) return aria;
      const labelled = (element.getAttribute("aria-labelledby") || "").split(/\s+/).filter(Boolean)
        .map((id) => { const node = document.getElementById(id); return node ? (node.innerText || node.textContent || "") : ""; })
        .join(" ");
      if (labelled.trim()) return labelled;
      if (element.labels && element.labels.length) {
        const labels = Array.from(element.labels).map((label) => label.innerText || label.textContent || "").join(" ");
        if (labels.trim()) return labels;
      }
      for (const attribute of ["alt", "title", "placeholder"]) {
        const value = (element.getAttribute(attribute) || "").trim();
        if (value) return value;
      }
      return element.innerText || element.textContent || "";
    };
    const roleOf = (element) => {
      const explicit = (element.getAttribute("role") || "").trim();
      if (explicit) return explicit.split(/\s+/)[0];
      const tag = element.tagName.toLowerCase();
      if (tag === "a") return "link";
      if (tag === "button" || tag === "summary") return "button";
      if (tag === "select") return "combobox";
      if (tag === "textarea" || element.isContentEditable) return "textbox";
      if (tag === "iframe") return "iframe";
      if (/^h[1-6]$/.test(tag)) return "heading";
      if (tag === "input") {
        const type = (element.getAttribute("type") || "text").toLowerCase();
        const roles = { checkbox: "checkbox", radio: "radio", range: "slider", file: "file", password: "password", submit: "button", reset: "button", button: "button" };
        return roles[type] || "textbox";
      }
      return "generic";
    };
    const valueOf = (element, role) => {
      if (role === "password" || role === "file") return null;
      const tag = element.tagName.toLowerCase();
      if (tag === "select") {
        const option = element.options[element.selectedIndex];
        return option ? clip(option.label || option.text || "", LIMIT.value) : null;
      }
      if (tag === "input") {
        const type = (element.type || "text").toLowerCase();
        if (type === "checkbox" || type === "radio") return null;
        return element.value == null ? null : clip(element.value, LIMIT.value);
      }
      if (tag === "textarea") return element.value == null ? null : clip(element.value, LIMIT.value);
      if (element.isContentEditable) return clip(element.innerText, LIMIT.value);
      return null;
    };
    const checkedOf = (element, role) => {
      const aria = element.getAttribute("aria-checked");
      if (aria === "true") return true;
      if (aria === "false") return false;
      if (role !== "checkbox" && role !== "radio" && role !== "switch") return null;
      return typeof element.checked === "boolean" ? element.checked : null;
    };
    const selector = "a[href],button,input,select,textarea,summary,iframe,[contenteditable],[tabindex],[role],h1,h2,h3,h4,h5,h6";
    const found = [];
    for (const element of document.querySelectorAll(selector)) {
      const tag = element.tagName.toLowerCase();
      if (tag === "input" && (element.getAttribute("type") || "").toLowerCase() === "hidden") continue;
      if (element.getAttribute("aria-hidden") === "true") continue;
      if (element.getClientRects().length === 0) continue;
      found.push(element);
    }
    const dropped = Math.max(0, found.length - LIMIT.controls);
    const controls = found.slice(0, LIMIT.controls).map((element, index) => {
      const role = roleOf(element);
      const name = clip(nameOf(element), LIMIT.name);
      refs.push(element);
      return {
        ref: index + 1,
        role,
        name: role === "iframe" ? name + " (not inspected)" : name,
        value: valueOf(element, role),
        focused: document.activeElement === element,
        disabled: Boolean(element.disabled) || element.getAttribute("aria-disabled") === "true",
        checked: checkedOf(element, role),
      };
    });
    const whole = String((document.body && document.body.innerText) || "").replace(/\s+/g, " ").trim();
    let text = whole.slice(0, LIMIT.text);
    let truncated = dropped > 0 || whole.length > LIMIT.text;
    if (dropped > 0) text = (text ? text + "\n" : "") + "\u2026 " + dropped + " more controls not shown";
    const result = { ok: true, url: location.href, title: document.title || "", controls, text, truncated };
    const bytes = (value) => new TextEncoder().encode(JSON.stringify(value)).length;
    while (bytes(result) > LIMIT.json) {
      if (result.text.length > 0) {
        result.text = result.text.slice(0, Math.floor(result.text.length / 2));
      } else if (result.controls.length > 0) {
        result.controls.pop();
      } else {
        result.title = "";
        break;
      }
      result.truncated = true;
    }
    return result;
  } catch (error) {
    return { ok: false, code: "failed", message: String(error && error.message || error) };
  }
})()"#;

#[cfg(test)]
mod tests {
    use super::{parse_info, parse_snapshot};
    use kg_domain::OpError;

    #[test]
    fn a_snapshot_keeps_its_refs_roles_and_flags() {
        let snapshot = parse_snapshot(
            r#"{"ok":true,"url":"https://example.com/login","title":"Sign in",
                "controls":[
                  {"ref":1,"role":"heading","name":"Sign in","value":null,"focused":false,"disabled":false,"checked":null},
                  {"ref":2,"role":"textbox","name":"Email","value":"ada@example.com","focused":true,"disabled":false,"checked":null},
                  {"ref":3,"role":"checkbox","name":"Remember me","value":null,"focused":false,"disabled":false,"checked":true}
                ],
                "text":"Sign in to continue.","truncated":false}"#,
        )
        .expect("a snapshot");
        assert_eq!(snapshot.url, "https://example.com/login");
        assert_eq!(snapshot.title, "Sign in");
        assert_eq!(snapshot.controls.len(), 3);
        assert_eq!(snapshot.controls[0].role, "heading");
        assert_eq!(snapshot.controls[1].ref_id, 2);
        assert_eq!(
            snapshot.controls[1].value.as_deref(),
            Some("ada@example.com")
        );
        assert!(snapshot.controls[1].focused);
        assert_eq!(snapshot.controls[2].checked, Some(true));
        assert!(!snapshot.truncated);
    }

    #[test]
    fn a_cut_snapshot_says_so() {
        let snapshot = parse_snapshot(
            r#"{"ok":true,"url":"https://example.com/","title":"",
                "controls":[],
                "text":"\u2026 12 more controls not shown","truncated":true}"#,
        )
        .expect("a snapshot");
        assert!(snapshot.truncated);
        assert!(snapshot.text.contains("12 more controls not shown"));
        assert!(snapshot.title.is_empty());
    }

    #[test]
    fn a_password_value_never_survives_the_parser() {
        let snapshot = parse_snapshot(
            r#"{"ok":true,"url":"https://example.com/","title":"",
                "controls":[
                  {"ref":1,"role":"password","name":"Password","value":"hunter2","focused":false,"disabled":false,"checked":null},
                  {"ref":2,"role":"textbox","name":"Email","value":"ada@example.com","focused":false,"disabled":false,"checked":null}
                ],
                "text":"","truncated":false}"#,
        )
        .expect("a snapshot");
        assert_eq!(snapshot.controls[0].value, None);
        assert_eq!(
            snapshot.controls[1].value.as_deref(),
            Some("ada@example.com")
        );
    }

    #[test]
    fn malformed_json_is_refused() {
        assert_eq!(
            parse_snapshot("{not json"),
            Err(OpError::Failed(
                "the page could not be read; take the page over or try again".into()
            ))
        );
    }

    #[test]
    fn an_empty_reply_is_refused() {
        assert_eq!(
            parse_snapshot(""),
            Err(OpError::Failed(
                "the page could not be read; take the page over or try again".into()
            ))
        );
        assert!(parse_info("   ").is_err());
    }

    #[test]
    fn a_refused_script_keeps_its_own_error() {
        assert_eq!(
            parse_snapshot(
                r#"{"ok":false,"code":"not-found","message":"ref 7 is not on the page"}"#
            ),
            Err(OpError::NotFound("ref 7 is not on the page".into()))
        );
    }
}
