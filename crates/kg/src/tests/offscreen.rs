use clap::Parser as _;

use crate::Cli;

#[test]
fn offscreen_flag_is_its_own_mode() {
    let cli = Cli::try_parse_from(["kg", "--offscreen"]).expect("the flag parses");
    assert!(cli.offscreen);
    assert!(!cli.headless);
}

#[test]
fn offscreen_and_headless_do_not_combine() {
    assert!(Cli::try_parse_from(["kg", "--offscreen", "--headless"]).is_err());
}
