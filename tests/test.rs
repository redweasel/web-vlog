use std::time::Duration;

use v_log::*;

#[test]
#[rustfmt::skip]
fn test_init() {
    // Choosing a fixed port is recommended if the VSCode links are used.
    // That is because otherwise one has to (re)allow opening links on every run.
    // The port is left out in this test however to avoid collisions.
    let port = web_vlog::init();

    let msgcol = Color::Healthy;
    
    // Test both variants of specifying color
    message!("early", color: msgcol, "Early message 1");
    message!("table1", color: Healthy, "Early message (Table 1)");

    // Open the browser (quickstart the debugging session, required `open` as dependency)
    let _ = open::that(format!("http://localhost:{port}/"));

    // Instead of opening the webbrowser, one can wait for the user to do so.
    //println!("waiting for connection on port {port}");
    //web_vlog::wait_for_connection();

    std::thread::sleep(Duration::from_millis(1000));

    // Also test no color
    message!("table2", "Test message (Table 2)");

    // Now draw a table with all the different styles
    let scale = 35.;
    // Table 1:
    // x: Point Style
    // y: Size/Color
    {
        use PointStyle::*;
        use Color::*;
        let colors = [Base, Healthy, Info, Warn, Error, X, Y, Z, Hex(0xFF00FFFF)];
        let offx = 20.;
        let offy = 50.;
        for (y, _) in colors.iter().enumerate() {
            let y = y as f64;
            polyline!("table1", ([offx, offy+y*scale], [offx+12.*scale, offy+y*scale]), 0.0, Base, "-");
        }
        for (x, point_style) in [Circle, FilledCircle, DashedCircle, Square, FilledSquare, DashedSquare, Point, PointOutline, PointSquare, PointSquareOutline, PointDiamond, PointDiamondOutline, PointCross].into_iter().enumerate() {
            let x = x as f64;
            polyline!("table1", ([offx+x*scale, offy], [offx+x*scale, offy+8.*scale]), 0.0, Base, "-");
            for (y, color) in colors.iter().copied().enumerate() {
                let size = (y + 2) as f64 * 3.;
                let y = y as f64;
                // Note, that for color one can not put an expr made up of multiple tokens...
                point!("table1", [offx+x*scale, offy+y*scale], size, color, point_style, "{y}");
            }
        }
    }
    // Table 2:
    // x: Line Type
    // y: Label Alignment and Size
    {
        use LineStyle::*;
        use Color::*;
        let offset = 11. * scale;
        let scale = 60.;
        let line_styles = [Simple, Dashed, Arrow, InsideHarpoonCCW, InsideHarpoonCW];
        let colors = [Base, Healthy, Info, Warn, Error];
        let alignments = [TextAlignment::Left, TextAlignment::Center, TextAlignment::Right, TextAlignment::Flexible];
        for (x, (line_style, color)) in line_styles.into_iter().zip(colors.into_iter()).enumerate() {
            let x = x as f64;
            for (y, align) in alignments.iter().copied().enumerate() {
                let size = (y + 2) as f64;
                let y = y as f64;
                polyline!("table2", ([x*scale, y*scale + offset], [(x + 1.)*scale, y*scale + offset]), size, color, line_style, "L {x},{y}");
                polyline!("table2", ([x*scale, (y + 0.5)*scale + offset], [(x + 1.)*scale, (y + 0.5)*scale + offset]), 1.0, Base, "-");
                polyline!("table2", ([(x + 0.5)*scale, (y + 0.6)*scale + offset], [(x + 0.5)*scale, (y + 0.4)*scale + offset]), 1.0, Base, "-");
                label!("table2", [(x + 0.5)*scale, (y + 0.5)*scale + offset], (x*4.+8., color, align), "{x}");
            }
        }
    }
    // Draw an animation of a loading symbol (simple performance test)
    for i in 0..=200 {
        let t = (i as f64) * 0.2;
        clear!("loading");
        for x in 0..40 {
            let x1 = (t + (x as f64)*0.1).cos() * 20. + 400.;
            let y1 = (t + (x as f64)*0.1).sin() * 20. + 400.;
            let x2 = (t + (x as f64 + 1.)*0.1).cos() * 20. + 400.;
            let y2 = (t + (x as f64 + 1.)*0.1).sin() * 20. + 400.;
            polyline!("loading", ([x1, y1], [x2, y2]), x as f64/3.+1., Info);
        }
        label!("loading", [400., 400.], (12., Base, Center), "{:.1}%", i as f64/2.);
        message!("loading", "{:.1}%", i as f64/2.);
        std::thread::sleep(Duration::from_millis(16));
    }

    for _ in 0..1000 {
        // check that these equal messages are combined
        message!("spam", "TEST SPAMMING!");
    }
    for i in 0..100 {
        // test that no html code injection is allowed and that the escape of ' works correctly...
        message!("spam", "TEST SPAMMING<img src=\"fun.gif\"onerror=\"alert('{}');this.remove();\"/>", "!".repeat(i));
        label!("spam", [(i%7)as f64*80.,(i%17)as f64*30.], "TEST SPAMMING<script>alert(\"{}\");</script>", "!".repeat(i/10));
    }
}
