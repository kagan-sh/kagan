use jiff::tz::{TimeZone, offset};

use super::reset_text;

#[test]
fn a_reset_is_written_in_the_local_zone_not_utc() {
    let ten_hours_east = TimeZone::fixed(offset(10));
    let five_hours_west = TimeZone::fixed(offset(-5));
    assert_eq!(reset_text(0, ten_hours_east), "Jan 1, 10:00 AM");
    assert_eq!(reset_text(0, five_hours_west), "Dec 31, 7:00 PM");
}
