use super::{Kind, accepts, kind};
use kg_domain::{OpError, prefs};
use serde_json::json;

#[test]
fn a_sidebar_width_is_refused_outside_its_range() {
    let left = format!("main.{}", prefs::LEFT_WIDTH);
    let right = format!("main.{}", prefs::RIGHT_WIDTH);
    let (left_min, left_max) = prefs::LEFT_WIDTH_RANGE;
    let (right_min, right_max) = prefs::RIGHT_WIDTH_RANGE;
    assert!(accepts(kind(&left).unwrap(), &left, &json!(left_min)).is_ok());
    assert!(accepts(kind(&left).unwrap(), &left, &json!(left_max)).is_ok());
    assert!(accepts(kind(&left).unwrap(), &left, &json!(left_max + 1.)).is_err());
    assert!(accepts(kind(&right).unwrap(), &right, &json!(right_min)).is_ok());
    assert!(accepts(kind(&right).unwrap(), &right, &json!(right_max)).is_ok());
    assert!(accepts(kind(&right).unwrap(), &right, &json!(right_min - 1.)).is_err());
    assert!(accepts(kind(&right).unwrap(), &right, &json!(99_999_999)).is_err());
}

#[test]
fn a_flag_error_says_true_or_false() {
    let error = accepts(Kind::Flag, "terminal-copy-on-select", &json!("on")).unwrap_err();
    assert!(
        matches!(&error, OpError::Invalid(message) if message.contains("true or false")),
        "{error}"
    );
}

#[test]
fn a_rail_hidden_error_shows_the_list_to_write() {
    let error = accepts(Kind::PluginIds, prefs::RAIL_HIDDEN, &json!("x")).unwrap_err();
    assert!(
        matches!(&error, OpError::Invalid(message) if message.contains("such as [\"")),
        "{error}"
    );
}
