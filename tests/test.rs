use std::time::Duration;

use v_log::*;

#[test]
#[rustfmt::skip]
fn test_init() {
    let _ = web_vlog::init();
    
    message!("", color: Healthy, "Early message");

    let _ = open::that("http://localhost:13700/");
    std::thread::sleep(Duration::from_millis(1000));

    message!("", "Test message");
    polyline!("", ([10., 10.], [10., 100.], [100., 100.], [100., 10.],), 2.0, Base, "--");
    point!("", [55., 55.], 90., Healthy, "--O", "Center");
    label!("", [200., 200.], (12., Healthy, "<"), "Outside");
    point!("", [145., 55.], 90., X, "O", "Filled");
    point!("", [55., 145.], 90., Y, "-O", "Outlined");
    message!("", color: Warn, "Finished");

    std::thread::sleep(Duration::from_millis(3000));
    clear!("");
}
