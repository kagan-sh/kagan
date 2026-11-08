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
    /// Set by the script for an `input` whose `type` is `password`.
    #[serde(default)]
    secret: bool,
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
            // A page sets its own role, so the script marks a password input by its type;
            // the host drops the value when the type or the role says password.
            let password = control.secret || control.role == "password";
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

#[cfg(desktop)]
pub(super) const INFO: &str = r#"(() => {
  try {
    return { ok: true, url: location.href, title: document.title || "", ready: document.readyState === "complete" };
  } catch (error) {
    return { ok: false, code: "failed", message: String(error && error.message || error) };
  }
})()"#;

#[cfg(desktop)]
pub(super) const SNAPSHOT: &str = r#"(() => {
  const LIMIT = { controls: 300, name: 120, value: 200, text: 8192, json: 24576 };
  const clip = (text, limit) => {
    const flat = String(text == null ? "" : text).replace(/\s+/g, " ").trim();
    return flat.length > limit ? flat.slice(0, limit) + "\u2026" : flat;
  };
  const marker = (count) => "\u2026 " + count + " more controls not shown";
  const size = (value) => new TextEncoder().encode(JSON.stringify(value)).length;
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
    // A password field is found by its input type, so an explicit role cannot make it readable.
    const secretOf = (element) =>
      element.tagName.toLowerCase() === "input" && (element.getAttribute("type") || "").toLowerCase() === "password";
    const valueOf = (element, role, secret) => {
      if (secret || role === "password" || role === "file") return null;
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
    let cut = Math.max(0, found.length - LIMIT.controls);
    const controls = found.slice(0, LIMIT.controls).map((element, index) => {
      const role = roleOf(element);
      const secret = secretOf(element);
      const name = clip(nameOf(element), LIMIT.name);
      refs.push(element);
      return {
        ref: index + 1,
        role,
        name: role === "iframe" ? name + " (not inspected)" : name,
        value: valueOf(element, role, secret),
        secret,
        focused: document.activeElement === element,
        disabled: Boolean(element.disabled) || element.getAttribute("aria-disabled") === "true",
        checked: checkedOf(element, role),
      };
    });
    const whole = String((document.body && document.body.innerText) || "").replace(/\s+/g, " ").trim();
    let text = whole.slice(0, LIMIT.text);
    const result = { ok: true, url: location.href, title: document.title || "", controls, text, truncated: cut > 0 || whole.length > LIMIT.text };
    let shortened = false;
    while (size(result) > LIMIT.json) {
      if (result.text.length > 0) {
        result.text = result.text.slice(0, Math.floor(result.text.length / 2));
        shortened = true;
      } else if (result.controls.length > 0) {
        result.controls.pop();
        cut += 1;
      } else {
        result.title = "";
        break;
      }
      result.truncated = true;
    }
    if (cut > 0 || shortened) {
      result.truncated = true;
      for (;;) {
        const suffix = cut > 0 ? marker(cut) : "\u2026 text cut at the size limit";
        const next = (result.text ? result.text + "\n" : "") + suffix;
        if (size({ ...result, text: next }) <= LIMIT.json) {
          result.text = next;
          break;
        }
        if (result.text.length > 0) {
          result.text = result.text.slice(0, Math.floor(result.text.length / 2));
        } else if (result.controls.length > 0) {
          result.controls.pop();
          cut += 1;
        } else {
          result.text = suffix;
          break;
        }
      }
    }
    return result;
  } catch (error) {
    return { ok: false, code: "failed", message: String(error && error.message || error) };
  }
})()"#;

#[cfg(test)]
mod tests;
