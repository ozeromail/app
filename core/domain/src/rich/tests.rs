use super::*;

fn styles(text: &str) -> Vec<(Style, &str)> {
    marks(text)
        .into_iter()
        .map(|m| (m.style, &text[m.content()]))
        .collect()
}

#[test]
fn reads_styles() {
    assert_eq!(
        styles("a **bold** and *it* or _it_ ~~no~~ `x*y*`"),
        vec![
            (Style::Bold, "bold"),
            (Style::Italic, "it"),
            (Style::Italic, "it"),
            (Style::Strike, "no"),
            (Style::Code, "x*y*"),
        ]
    );
    assert_eq!(
        styles("**a _b_ c**"),
        vec![(Style::Bold, "a _b_ c"), (Style::Italic, "b")]
    );
}

#[test]
fn leaves_plain_text_plain() {
    for text in [
        "snake_case_name",
        "2 * 3 * 4",
        "** not bold**",
        "a*",
        "**",
        "[not a link] (here)",
        "[spaced](a b)",
        "#nospace",
    ] {
        assert_eq!(styles(text), vec![], "{text}");
    }
}

#[test]
fn reads_links_and_lines() {
    let text = "# Plan\n- one **two**\n> said\nsee [the map](https://maps.example)";
    let marks = marks(text);
    let seen: Vec<_> = marks
        .iter()
        .map(|m| (m.style, &text[m.content()]))
        .collect();
    assert_eq!(
        seen,
        vec![
            (Style::Heading, "Plan"),
            (Style::Item, "one **two**"),
            (Style::Bold, "two"),
            (Style::Quote, "said"),
            (Style::Link, "the map"),
        ]
    );
    assert_eq!(marks[4].url.as_deref(), Some("https://maps.example"));
}

#[test]
fn escapes_marks() {
    let plain = "a *b* _c_ # [d](e) \\ ![](file:1)";
    let escaped = escape(plain);
    assert!(marks(&escaped).iter().all(|m| m.style == Style::Escape));
    assert_eq!(to_plain(&escaped, &|_| None), plain);
    assert!(!is_rich(&escaped));
    assert!(is_rich("**a**"));
}

#[test]
fn splits_into_blocks() {
    let text = "Hi\n![](file:3)\n![](file:4 \"s\")\nbye\n";
    assert_eq!(
        blocks(text),
        vec![
            Block::Text(0..2),
            Block::Image {
                line: 3..14,
                file: FileId(3),
                size: ImageSize::Full
            },
            Block::Image {
                line: 15..30,
                file: FileId(4),
                size: ImageSize::Small
            },
            Block::Text(31..35),
        ]
    );
    assert_eq!(&text[31..35], "bye\n");
    assert_eq!(image("![](file:4 \"x\")"), None);
    assert_eq!(image("![](file:)"), None);
    assert_eq!(blocks(""), vec![Block::Text(0..0)]);
}

#[test]
fn writes_plain_text() {
    let text = "# Plan\n- **one**\n- [map](https://m.example)\n> _said_\n![](file:7)\n![](file:8)\nvisit [https://a.example](https://a.example)";
    let name = |f: FileId| (f.0 == 7).then(|| "IMG_1.jpg".to_owned());
    assert_eq!(
        to_plain(text, &name),
        "Plan\n• *one*\n• map (https://m.example)\n> _said_\n[IMG_1.jpg]\nvisit https://a.example"
    );
    assert_eq!(shown("# Plan\n![](file:7)\n**a** b"), "Plan\na b");
}

#[test]
fn writes_html() {
    let text = "# Hi <you>\n- **one**\n- two\n\n> q\n![](file:7 \"m\")\nsee [map](example.com) or [x](javascript:void)";
    let image = |f: FileId| {
        (f.0 == 7).then(|| HtmlImage {
            cid: "f7.a@b".into(),
            name: "a\"b.jpg".into(),
        })
    };
    assert_eq!(
        to_html(text, &image),
        "<div dir=\"auto\"><h3 style=\"margin:0\">Hi &lt;you&gt;</h3>\
         <ul style=\"margin:0;padding-left:1.4em\"><li><b>one</b></li><li>two</li></ul>\
         <div><br></div>\
         <blockquote style=\"margin:0;padding-left:1ex;border-left:3px solid #d0d7e2;color:#5d6a82\"><div>q</div></blockquote>\
         <div><img src=\"cid:f7.a@b\" alt=\"a&quot;b.jpg\" style=\"width:66%;max-width:100%;height:auto\"></div>\
         <div>see <a href=\"https://example.com\">map</a> or <a href=\"#\">x</a></div></div>"
    );
}

#[test]
fn toggles_styles() {
    let (text, sel) = toggle("say hello now", 4, 9, Style::Bold, None);
    assert_eq!((text.as_str(), sel.clone()), ("say **hello** now", 6..11));
    let (text, sel) = toggle(&text, sel.start, sel.end, Style::Bold, None);
    assert_eq!((text.as_str(), sel), ("say hello now", 4..9));
    // Spaces at the ends stay out.
    let (text, _) = toggle("say hello now", 3, 10, Style::Italic, None);
    assert_eq!(text, "say *hello* now");
    let (text, sel) = toggle("go here", 3, 7, Style::Link, Some("https://x.example"));
    assert_eq!((text.as_str(), sel), ("go [here](https://x.example)", 4..8));
    let (text, _) = toggle(&text, 4, 8, Style::Link, Some("https://y.example"));
    assert_eq!(text, "go [here](https://y.example)");
    let (text, _) = toggle(&text, 4, 8, Style::Link, None);
    assert_eq!(text, "go here");
    let (text, sel) = toggle("go ", 3, 3, Style::Link, Some("https://x.example"));
    assert_eq!(&text[sel], "https://x.example");
}

#[test]
fn toggles_lines() {
    let (text, sel) = toggle("one\ntwo\nthree", 1, 5, Style::Item, None);
    assert_eq!((text.as_str(), sel), ("- one\n- two\nthree", 3..9));
    let (text, sel) = toggle(&text, 3, 9, Style::Item, None);
    assert_eq!((text.as_str(), sel), ("one\ntwo\nthree", 1..5));
    let (text, sel) = toggle("- one", 3, 3, Style::Heading, None);
    assert_eq!((text.as_str(), sel), ("# one", 3..3));
}

#[test]
fn backspace_over_hidden_marks_takes_the_shown_character() {
    // The caret after `**bold**`: the hidden `*` goes, so `d` does instead.
    let fixed = fix("a **bold** b", "a **bold* b").unwrap();
    assert_eq!(
        fixed,
        Edit {
            text: "a **bol** b".into(),
            caret: 7
        }
    );
    // The last character of a mark: the mark goes with it.
    let fixed = fix("a **x** b", "a **x* b").unwrap();
    assert_eq!(
        fixed,
        Edit {
            text: "a  b".into(),
            caret: 2
        }
    );
}

#[test]
fn cutting_into_a_mark_takes_all_of_it() {
    // Backspace at the start of `bold` takes the space before it.
    let fixed = fix("a **bold** b", "a *bold** b").unwrap();
    assert_eq!(fixed.text, "a**bold** b");
    // Choosing and deleting across a mark's end.
    let fixed = fix("**bold** tail", "**bo tail").unwrap();
    assert_eq!(fixed.text, "bo tail");
    assert_eq!(fix("**bold**", "**bod**"), None);
}

#[test]
fn backspace_at_a_heading_makes_it_plain() {
    let fixed = fix("one\n# Two", "one\n#Two").unwrap();
    assert_eq!(
        fixed,
        Edit {
            text: "one\nTwo".into(),
            caret: 4
        }
    );
}

#[test]
fn line_break_goes_on_with_a_list() {
    let fixed = fix("- one", "- one\n").unwrap();
    assert_eq!(
        fixed,
        Edit {
            text: "- one\n- ".into(),
            caret: 8
        }
    );
    let fixed = fix("- one\n- ", "- one\n- \n").unwrap();
    assert_eq!(
        fixed,
        Edit {
            text: "- one\n".into(),
            caret: 6
        }
    );
    assert_eq!(fix("one", "one\n"), None);
    assert_eq!(fix("one", "oxne"), None);
}

#[test]
fn a_pasted_address_becomes_a_link() {
    let fixed = fix("see ", "see https://a.example/x").unwrap();
    assert_eq!(fixed.text, "see [https://a.example/x](https://a.example/x)");
    assert_eq!(fixed.caret, fixed.text.len());
    assert_eq!(fix("`a`", "`ahttps://a.example`"), None);
    assert_eq!(fix("see ", "see https://a b"), None);
}

#[test]
fn places_and_moves_images() {
    let text = insert_images("one\ntwo", 1, &[FileId(5)]);
    assert_eq!(text, "one\n![](file:5)\ntwo");
    let text = insert_images("one", 3, &[FileId(5), FileId(6)]);
    assert_eq!(text, "one\n![](file:5 \"m\")\n![](file:6 \"m\")\n");
    assert_eq!(remove_image(&text, FileId(5)), "one\n![](file:6 \"m\")\n");
    assert_eq!(
        change_image(&text, FileId(5), FileId(9), ImageSize::Small),
        "one\n![](file:9 \"s\")\n![](file:6 \"m\")\n"
    );

    let text = "a\n\nb1\nb2\n![](file:1)\n\nc";
    assert_eq!(
        move_image(text, FileId(1), true),
        "a\n\n![](file:1)\nb1\nb2\n\nc"
    );
    assert_eq!(
        move_image(text, FileId(1), false),
        "a\n\nb1\nb2\n\nc\n![](file:1)"
    );
    assert_eq!(
        move_image("![](file:1)\nx", FileId(1), true),
        "![](file:1)\nx"
    );
    assert_eq!(
        move_image("x\n![](file:1)", FileId(1), false),
        "x\n![](file:1)"
    );
    assert_eq!(
        move_image("![](file:1)\n![](file:2)", FileId(1), false),
        "![](file:2)\n![](file:1)"
    );
}

#[test]
fn finds_the_link_chosen() {
    let text = "see [the plan](https://a.example) now";
    assert_eq!(link_at(text, 5, 9).as_deref(), Some("https://a.example"));
    assert_eq!(link_at(text, 0, 3), None);
}
