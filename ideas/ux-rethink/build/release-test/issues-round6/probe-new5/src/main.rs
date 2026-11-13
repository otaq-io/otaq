// NEW-5 probe: spawn Codex in a ConPTY, type the bytes Otaq types, report whether the turn starts.
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type W = Arc<Mutex<Box<dyn Write + Send>>>;

fn screen(p: &Arc<Mutex<vt100::Parser>>) -> String {
    p.lock().unwrap().screen().contents()
}
fn dump(label: &str, s: &str) {
    println!("===== {label} =====");
    for l in s.lines().filter(|l| !l.trim().is_empty()) {
        println!("| {l}");
    }
}
fn w(wr: &W, b: &[u8]) {
    let mut g = wr.lock().unwrap();
    g.write_all(b).unwrap();
    g.flush().unwrap();
}
fn ms(n: u64) {
    std::thread::sleep(Duration::from_millis(n));
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (case, repo, codex, stub) = (&a[1], &a[2], &a[3], &a[4]);
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize { rows: 40, cols: 120, pixel_width: 0, pixel_height: 0 })
        .unwrap();
    let mut cmd = CommandBuilder::new(codex);
    cmd.arg("--no-daemon");
    cmd.arg("-c");
    cmd.arg("check_for_update_on_startup=false");
    cmd.arg("-c");
    cmd.arg(format!("projects.{repo}.trust_level=\"trusted\""));
    cmd.cwd(repo);
    let path = std::env::var("PATH").unwrap_or_default();
    cmd.env("PATH", format!("{stub};{path}"));
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let writer: W = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
    let parser = Arc::new(Mutex::new(vt100::Parser::new(40, 120, 0)));
    {
        let parser = parser.clone();
        let writer = writer.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                let n = match reader.read(&mut buf) { Ok(0) | Err(_) => break, Ok(n) => n };
                let chunk = &buf[..n];
                let mut p = parser.lock().unwrap();
                p.process(chunk);
                if chunk.windows(4).any(|x| x == b"\x1b[6n") {
                    let (r, c) = p.screen().cursor_position();
                    drop(p);
                    w(&writer, format!("\x1b[{};{}R", r + 1, c + 1).as_bytes());
                }
            }
        });
    }
    // Wait for the composer.
    let start = Instant::now();
    loop {
        let s = screen(&parser);
        if s.contains("for shortcuts") || s.contains("context left") || s.contains("% left") {
            break;
        }
        if start.elapsed() > Duration::from_secs(60) {
            dump("TIMEOUT waiting for composer", &s);
            let _ = child.kill();
            return;
        }
        ms(250);
    }
    ms(3000);
    dump("idle", &screen(&parser));
    let end: &[u8] = b"\x1b[F";
    let text: &str = match case.as_str() {
        "push" | "push_twice" => "$otaq:push",
        "push_space" => "$otaq:push ",
        "restack" => "$otaq:restack",
        "restack_told" => "$otaq:restack SHOP-1's merge check failed. Do nothing; reply only with the word OK.",
        "plain" => "Do nothing; reply only with the word PONG.",
        _ => panic!("case"),
    };
    // What Otaq's type_and_submit writes for Codex on Windows: text, 80 ms, End, 100 ms, Enter.
    w(&writer, text.as_bytes());
    ms(80);
    w(&writer, end);
    ms(100);
    dump("before Enter (t=180ms)", &screen(&parser));
    w(&writer, b"\r");
    if case == "push_twice" {
        ms(300);
        dump("after first Enter (+300ms)", &screen(&parser));
        w(&writer, b"\r");
    }
    ms(2500);
    let s1 = screen(&parser);
    dump("after Enter +2.5s", &s1);
    ms(4000);
    let s2 = screen(&parser);
    dump("after Enter +6.5s", &s2);
    let working = |s: &str| s.contains("Working") || s.contains("esc to interrupt") || s.contains("Esc to interrupt");
    let still = s2.lines().any(|l| l.trim_start().starts_with('›') && l.contains(text.trim_end()));
    println!("RESULT case={case} turn_started={} text_still_in_line={}", working(&s1) || working(&s2), still);
    // Stop any turn, then quit.
    w(&writer, b"\x1b");
    ms(1500);
    let _ = child.kill();
}
