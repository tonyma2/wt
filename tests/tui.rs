pub mod common;

use common::*;

#[test]
fn no_args_requires_tty() {
    let (home, _repo) = setup();
    let output = run_wt(home.path(), |_| {});
    assert_error(
        &output,
        1,
        "cannot launch picker, stdout is not a terminal\n",
    );
}
