//! The closed captions of a transport stream, each time they change:
//! `cargo run -p strata-mv-core --example cc-dump -- file.ts [channel]`.

use std::io::Read;

use strata_mv_core::captions::{Decoder, Extractor};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: cc-dump file.ts [cc1|cc2|cc3|cc4|s1…s6]");
        std::process::exit(2);
    };
    let only = args.next();
    let mut f = std::fs::File::open(&path).expect("open");
    let (mut x, mut d) = (Extractor::new(), Decoder::new());
    let mut shown = std::collections::BTreeMap::new();
    let mut first = None;
    let mut buf = vec![0u8; 188 * 512];
    let mut pictures = Vec::new();
    loop {
        let n = f.read(&mut buf).expect("read");
        if n == 0 {
            x.flush(&mut pictures);
        } else {
            pictures.extend(x.feed(&buf[..n]));
        }
        for (pts, cc) in pictures.drain(..) {
            d.feed(&cc);
            let t0 = *first.get_or_insert(pts);
            for (ch, s) in d.screens(true) {
                if only.as_ref().is_some_and(|o| *o != ch) || shown.get(&ch) == Some(&s) {
                    continue;
                }
                println!("{:7.3} s  {ch}:", (pts - t0) as f64 / 90_000.0);
                for l in &s.lines {
                    let text: String = l.spans.iter().map(|s| s.text.as_str()).collect();
                    let look: Vec<String> = l
                        .spans
                        .iter()
                        .map(|s| {
                            format!(
                                "{}{}{}",
                                s.color,
                                if s.italic { " i" } else { "" },
                                if s.underline { " u" } else { "" }
                            )
                        })
                        .collect();
                    println!(
                        "    row {:2} col {:2}  {text:<34} {}",
                        l.row,
                        l.col,
                        look.join(" | ")
                    );
                }
                shown.insert(ch, s);
            }
        }
        if n == 0 {
            break;
        }
    }
    println!("{} pictures", x.pictures);
}
