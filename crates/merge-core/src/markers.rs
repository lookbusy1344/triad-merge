//! Reading a file's conflict markers back into a merge's inputs (spec §7.4,
//! `PLAN.md` §D5.7).

use crate::document::Labels;
use crate::merge3::Inputs;
use crate::text::Text;

/// A file's conflicts as a merge's inputs.
#[derive(Debug)]
pub struct Parsed {
    pub inputs: Inputs,
    /// From the first conflict's marker lines.
    pub labels: Labels,
    pub marker_length: usize,
}

/// Why a file's markers do not give a merge. Lines count from 1.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MarkerError {
    #[error("no conflict markers")]
    NoMarkers,
    #[error("line {line}: the conflict is not closed")]
    Unclosed { line: usize },
    #[error("line {line}: a conflict starts inside another")]
    Nested { line: usize },
    #[error("line {line}: the marker is out of order")]
    OutOfOrder { line: usize },
    #[error("line {line}: the conflict ends before its =======")]
    NoSeparator { line: usize },
    #[error("line {line}: a diff line starts with neither ' ', '-' nor '+'")]
    BadDiffLine { line: usize },
    #[error(
        "line {line}: a conflict needs at least two sides and one base fewer, got {sides} sides and {bases} bases"
    )]
    Shape { line: usize, sides: usize, bases: usize },
    #[error("line {line}: a conflict of {found} sides after conflicts of {expected}")]
    MixedArity { line: usize, expected: usize, found: usize },
}

/// The shortest conflict marker git and jj write or read.
pub const MIN_MARKER_LENGTH: usize = 7;

/// The fewest sides a conflict has.
const MIN_SIDES: usize = 2;

/// jj's note on a section whose last line has no newline.
const NO_EOL_NOTE: &str = "(no terminating newline)";

/// Read the conflicts in `text`. The marker length is the longest run of `<` opening a line
/// at which the markers parse; every marker has exactly that length, so a writer's longer
/// markers keep shorter marker-like lines inside as content.
///
/// # Errors
///
/// [`MarkerError::NoMarkers`] when no line opens a conflict, and otherwise the error at the
/// longest marker length when no length parses.
///
/// # Panics
///
/// Never: every conflict that parses has at least two sides and one base fewer.
pub fn parse(text: &Text) -> Result<Parsed, MarkerError> {
    let content = text.content();
    let mut lengths: Vec<usize> = content
        .split_inclusive('\n')
        .filter_map(|line| marker(line, None))
        .filter(|(kind, _, _)| *kind == Kind::Start)
        .map(|(_, len, _)| len)
        .collect();
    lengths.sort_unstable_by(|a, b| b.cmp(a));
    lengths.dedup();
    let mut first_error = None;
    for len in lengths {
        match parse_at(content, len) {
            Ok((sides, bases, labels)) => {
                let texts = |texts: Vec<String>| -> Vec<Text> {
                    texts.into_iter().map(|t| Text::new(text.encoding(), t)).collect()
                };
                #[expect(
                    clippy::expect_used,
                    reason = "invariant: `parse_at` gives n > 1 sides and n - 1 bases"
                )]
                let inputs = Inputs::new(texts(sides), texts(bases))
                    .expect("every conflict has at least two sides and one base fewer");
                return Ok(Parsed { inputs, labels, marker_length: len });
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    Err(first_error.unwrap_or(MarkerError::NoMarkers))
}

/// Whether a line of `content` opens a conflict: a run of at least [`MIN_MARKER_LENGTH`]
/// `<`, then whitespace or the line's end.
#[must_use]
pub fn has_conflict_start(content: &str) -> bool {
    content
        .split_inclusive('\n')
        .any(|line| marker(line, None).is_some_and(|(kind, _, _)| kind == Kind::Start))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Start,
    End,
    /// git's `|||||||`.
    Ancestor,
    /// git's `=======`.
    Separator,
    Add,
    Remove,
    Diff,
    Note,
}

/// The marker `line` holds: its kind, length and label. A marker is a run of one marker
/// character, `len` long (at least [`MIN_MARKER_LENGTH`] without `len`), then whitespace or
/// the line's end.
fn marker(line: &str, len: Option<usize>) -> Option<(Kind, usize, &str)> {
    let first = *line.as_bytes().first()?;
    let kind = match first {
        b'<' => Kind::Start,
        b'>' => Kind::End,
        b'|' => Kind::Ancestor,
        b'=' => Kind::Separator,
        b'+' => Kind::Add,
        b'-' => Kind::Remove,
        b'%' => Kind::Diff,
        b'\\' => Kind::Note,
        _ => return None,
    };
    let run = line.bytes().take_while(|&b| b == first).count();
    let rest = &line[run..];
    let fits = len.map_or(run >= MIN_MARKER_LENGTH, |len| run == len);
    let ends = rest.bytes().next().is_none_or(|b| b.is_ascii_whitespace());
    (fits && ends).then_some((kind, run, rest.trim()))
}

/// A label as a pane title: jj's note and numbered defaults removed.
fn label(text: &str) -> String {
    let text = text.strip_suffix(NO_EOL_NOTE).unwrap_or(text).trim_end();
    let numbered = |role: &str| {
        text.strip_prefix(role)
            .and_then(|n| n.strip_prefix(" #"))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    if text == "base" || numbered("base") || numbered("side") {
        String::new()
    } else {
        text.to_owned()
    }
}

/// One conflict's sections and labels. A git conflict without a base has no base section.
struct Hunk {
    sides: Vec<String>,
    bases: Vec<String>,
    side_labels: Vec<String>,
    base_labels: Vec<String>,
}

type Sections = (Vec<String>, Vec<String>, Labels);

/// A conflict whose end marker has not come yet.
struct Open<'a> {
    /// The start marker's line number.
    start: usize,
    /// The start marker's byte offset.
    offset: usize,
    /// The start marker's line.
    marker: &'a str,
    /// The conflict's lines so far after the start marker, numbered.
    body: Vec<(usize, &'a str)>,
}

/// [`parse`] at one marker length: each side's and base's whole text, and the labels.
fn parse_at(content: &str, len: usize) -> Result<Sections, MarkerError> {
    let mut sides: Vec<String> = Vec::new();
    let mut bases: Vec<String> = Vec::new();
    let mut labels = None;
    let mut plain_start = 0;
    let mut open: Option<Open> = None;
    let mut offset = 0;
    for (index, line) in content.split_inclusive('\n').enumerate() {
        let number = index + 1;
        let kind = marker(line, Some(len)).map(|(kind, _, _)| kind);
        match (&mut open, kind) {
            (None, Some(Kind::Start)) => {
                open = Some(Open { start: number, offset, marker: line, body: Vec::new() });
            }
            (None, _) => {}
            (Some(_), Some(Kind::Start)) => return Err(MarkerError::Nested { line: number }),
            (Some(conflict), Some(Kind::End)) => {
                let hunk = hunk(conflict, (number, line), len)?;
                let plain = &content[plain_start..conflict.offset];
                if labels.is_none() {
                    let count = hunk.sides.len();
                    sides = vec![String::new(); count];
                    bases = vec![String::new(); count - 1];
                    labels = Some(Labels::new(
                        hunk.side_labels.clone(),
                        pad(&hunk.base_labels, count - 1),
                    ));
                } else if hunk.sides.len() != sides.len() {
                    return Err(MarkerError::MixedArity {
                        line: conflict.start,
                        expected: sides.len(),
                        found: hunk.sides.len(),
                    });
                }
                for (text, section) in sides.iter_mut().zip(&hunk.sides) {
                    text.push_str(plain);
                    text.push_str(section);
                }
                for (i, text) in bases.iter_mut().enumerate() {
                    text.push_str(plain);
                    if let Some(section) = hunk.bases.get(i) {
                        text.push_str(section);
                    }
                }
                plain_start = offset + line.len();
                open = None;
            }
            (Some(conflict), _) => conflict.body.push((number, line)),
        }
        offset += line.len();
    }
    if let Some(conflict) = open {
        return Err(MarkerError::Unclosed { line: conflict.start });
    }
    let labels = labels.ok_or(MarkerError::NoMarkers)?;
    let rest = &content[plain_start..];
    for text in sides.iter_mut().chain(bases.iter_mut()) {
        text.push_str(rest);
    }
    Ok((sides, bases, labels))
}

/// `labels`, then empty labels up to `count`.
fn pad(labels: &[String], count: usize) -> Vec<String> {
    let mut padded = labels.to_vec();
    padded.resize(count, String::new());
    padded
}

/// A conflict from its start marker and body in `open` and its end marker, numbered.
fn hunk(open: &Open, (end, end_line): (usize, &str), len: usize) -> Result<Hunk, MarkerError> {
    let &Open { start, marker: start_line, ref body, .. } = open;
    let jj = body.first().is_some_and(|(_, line)| {
        matches!(marker(line, Some(len)), Some((Kind::Add | Kind::Remove | Kind::Diff, _, _)))
    });
    let mut hunk = if jj {
        jj_hunk(start, body, len)?
    } else {
        git_hunk(start_line, body, (end, end_line), len)?
    };
    if !end_line.ends_with('\n') {
        let crlf = start_line.ends_with("\r\n");
        for section in hunk.sides.iter_mut().chain(hunk.bases.iter_mut()) {
            if section.ends_with('\n') {
                section.pop();
                if crlf && section.ends_with('\r') {
                    section.pop();
                }
            }
        }
    }
    Ok(hunk)
}

/// git's styles: left, an optional `|||||||` base, `=======`, right.
fn git_hunk(
    start_line: &str,
    body: &[(usize, &str)],
    (end, end_line): (usize, &str),
    len: usize,
) -> Result<Hunk, MarkerError> {
    #[derive(PartialEq)]
    enum State {
        Left,
        Base,
        Right,
    }
    let (mut left, mut base, mut right) = (String::new(), None::<String>, String::new());
    let mut base_label = String::new();
    let mut state = State::Left;
    for &(number, line) in body {
        match (marker(line, Some(len)), &state) {
            (Some((Kind::Ancestor, _, text)), State::Left) => {
                state = State::Base;
                base = Some(String::new());
                base_label = label(text);
            }
            (Some((Kind::Separator, _, _)), State::Left | State::Base) => state = State::Right,
            (Some((Kind::Ancestor | Kind::Separator, _, _)), _) => {
                return Err(MarkerError::OutOfOrder { line: number });
            }
            (_, State::Left) => left.push_str(line),
            (_, State::Base) => base.get_or_insert_default().push_str(line),
            (_, State::Right) => right.push_str(line),
        }
    }
    if state != State::Right {
        return Err(MarkerError::NoSeparator { line: end });
    }
    let side_label = |line: &str| marker(line, Some(len)).map_or_else(String::new, |m| label(m.2));
    Ok(Hunk {
        side_labels: vec![side_label(start_line), side_label(end_line)],
        base_labels: if base.is_some() { vec![base_label] } else { Vec::new() },
        sides: vec![left, right],
        bases: base.into_iter().collect(),
    })
}

/// jj's styles: `+++++++` before a side, `-------` before a base, `%%%%%%%` before a diff from
/// a base to a side.
fn jj_hunk(start: usize, body: &[(usize, &str)], len: usize) -> Result<Hunk, MarkerError> {
    enum Section {
        Side,
        Base,
        Diff,
    }
    let mut hunk = Hunk {
        sides: Vec::new(),
        bases: Vec::new(),
        side_labels: Vec::new(),
        base_labels: Vec::new(),
    };
    let mut section = Section::Side;
    for &(number, line) in body {
        match marker(line, Some(len)) {
            Some((Kind::Add, _, text)) => {
                section = Section::Side;
                hunk.sides.push(String::new());
                hunk.side_labels.push(label(text));
            }
            Some((Kind::Remove, _, text)) => {
                section = Section::Base;
                hunk.bases.push(String::new());
                hunk.base_labels.push(label(text));
            }
            Some((Kind::Diff, _, text)) => {
                section = Section::Diff;
                hunk.sides.push(String::new());
                hunk.bases.push(String::new());
                let from = text.strip_prefix("diff from:").map_or("", str::trim);
                hunk.base_labels.push(label(from));
                hunk.side_labels.push(String::new());
            }
            Some((Kind::Note, _, text)) => {
                if let (Section::Diff, Some(to), Some(last)) =
                    (&section, text.strip_prefix("to:"), hunk.side_labels.last_mut())
                {
                    *last = label(to.trim());
                }
            }
            _ => {
                let (side, base) = (hunk.sides.last_mut(), hunk.bases.last_mut());
                match (&section, side, base) {
                    (Section::Side, Some(side), _) => side.push_str(line),
                    (Section::Base, _, Some(base)) => base.push_str(line),
                    (Section::Diff, Some(side), Some(base)) => {
                        diff_line(line, side, base)
                            .ok_or(MarkerError::BadDiffLine { line: number })?;
                    }
                    _ => unreachable!("a jj conflict's body opens with a section marker"),
                }
            }
        }
    }
    if hunk.sides.len() >= MIN_SIDES && hunk.sides.len() == hunk.bases.len() + 1 {
        Ok(hunk)
    } else {
        Err(MarkerError::Shape { line: start, sides: hunk.sides.len(), bases: hunk.bases.len() })
    }
}

/// One line of a jj diff onto its side and base. An empty line counts as context: editors
/// strip the space from ` \n`.
fn diff_line(line: &str, side: &mut String, base: &mut String) -> Option<()> {
    if line == "\n" || line == "\r\n" {
        side.push_str(line);
        base.push_str(line);
        return Some(());
    }
    let rest = line.get(1..)?;
    match line.as_bytes().first()? {
        b' ' => {
            side.push_str(rest);
            base.push_str(rest);
        }
        b'-' => base.push_str(rest),
        b'+' => side.push_str(rest),
        _ => return None,
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Encoding;

    fn parsed(file: &str) -> Parsed {
        parse(&Text::decode(file.as_bytes()).expect("text")).unwrap_or_else(|e| panic!("{e}"))
    }

    fn error(file: &str) -> MarkerError {
        parse(&Text::decode(file.as_bytes()).expect("text")).expect_err("malformed")
    }

    fn content(texts: &[Text]) -> Vec<&str> {
        texts.iter().map(Text::content).collect()
    }

    fn texts(parsed: &Parsed) -> (Vec<&str>, Vec<&str>) {
        (content(parsed.inputs.sides()), content(parsed.inputs.bases()))
    }

    fn labels(sides: &[&str], bases: &[&str]) -> Labels {
        let owned = |l: &[&str]| l.iter().map(|&s| s.to_owned()).collect();
        Labels::new(owned(sides), owned(bases))
    }

    #[test]
    fn a_line_of_seven_or_more_lt_then_a_space_or_the_end_opens_a_conflict() {
        assert!(has_conflict_start("a\n<<<<<<< main\nb\n"));
        assert!(has_conflict_start("<<<<<<<\n"));
        assert!(has_conflict_start("<<<<<<<"));
        assert!(has_conflict_start("<<<<<<<<<<< conflict 1 of 1\n"));
        assert!(has_conflict_start("<<<<<<<\tlabel\r\n"));
    }

    #[test]
    fn short_runs_inner_runs_and_runs_joined_to_text_open_no_conflict() {
        assert!(!has_conflict_start(""));
        assert!(!has_conflict_start("<<<<<< six\n"));
        assert!(!has_conflict_start("a <<<<<<< main\n"));
        assert!(!has_conflict_start("<<<<<<<main\n"));
        assert!(!has_conflict_start("=======\n>>>>>>> right\n"));
    }

    #[test]
    fn git_merge_style_has_no_base_section() {
        let p = parsed("a\n<<<<<<< HEAD\nL\n=======\nR\n>>>>>>> topic\nz\n");
        assert_eq!(texts(&p), (vec!["a\nL\nz\n", "a\nR\nz\n"], vec!["a\nz\n"]));
        assert_eq!(p.labels, labels(&["HEAD", "topic"], &[""]));
        assert_eq!(p.marker_length, 7);
    }

    #[test]
    fn git_diff3_style_gives_the_base_and_its_label() {
        let p = parsed("a\n<<<<<<< ours\nL\n||||||| anc\nB\n=======\nR\n>>>>>>> theirs\nz\n");
        assert_eq!(texts(&p), (vec!["a\nL\nz\n", "a\nR\nz\n"], vec!["a\nB\nz\n"]));
        assert_eq!(p.labels, labels(&["ours", "theirs"], &["anc"]));
    }

    #[test]
    fn bare_markers_give_no_labels() {
        let p = parsed("<<<<<<<\nL\n|||||||\nB\n=======\nR\n>>>>>>>\n");
        assert_eq!(texts(&p), (vec!["L\n", "R\n"], vec!["B\n"]));
        assert_eq!(p.labels, labels(&["", ""], &[""]));
    }

    #[test]
    fn jj_git_style_default_labels_count_as_none() {
        let p = parsed("<<<<<<< side #1\nL\n||||||| base\nB\n=======\nR\n>>>>>>> side #2\n");
        assert_eq!(p.labels, labels(&["", ""], &[""]));
    }

    #[test]
    fn an_empty_section_is_an_empty_text() {
        let p = parsed("x\n<<<<<<< a\nL\n||||||| b\n=======\n>>>>>>> c\n");
        assert_eq!(texts(&p), (vec!["x\nL\n", "x\n"], vec!["x\n"]));
    }

    #[test]
    fn text_before_between_and_after_conflicts_reaches_every_input() {
        let p = parsed(
            "a\n<<<<<<<\nL1\n|||||||\nB1\n=======\nR1\n>>>>>>>\nm\n\
             <<<<<<<\nL2\n|||||||\nB2\n=======\nR2\n>>>>>>>\nz\n",
        );
        assert_eq!(
            texts(&p),
            (vec!["a\nL1\nm\nL2\nz\n", "a\nR1\nm\nR2\nz\n"], vec!["a\nB1\nm\nB2\nz\n"])
        );
    }

    #[test]
    fn jj_snapshot_style_with_three_sides() {
        let p = parsed(
            "<<<<<<< conflict 1 of 1\n+++++++ side #1\none\n------- base #1\na\n\
             +++++++ side #2\ntwo\n------- base #2\na\n+++++++ side #3\nthree\n\
             >>>>>>> conflict 1 of 1 ends\n",
        );
        assert_eq!(texts(&p), (vec!["one\n", "two\n", "three\n"], vec!["a\n", "a\n"]));
        assert_eq!(p.labels, labels(&["", "", ""], &["", ""]));
    }

    #[test]
    fn a_default_label_needs_a_number_after_the_hash() {
        let p = parsed("<<<<<<< side #\nL\n||||||| base #2b\nB\n=======\nR\n>>>>>>> side #x\n");
        assert_eq!(p.labels, labels(&["side #", "side #x"], &["base #2b"]));
    }

    #[test]
    fn jj_snapshot_style_labels() {
        let p = parsed(
            "<<<<<<< conflict 1 of 1\n+++++++ kx 1a \"left\"\nL\n------- ba 2b \"base\"\nB\n\
             +++++++ rr 3c \"right\"\nR\n>>>>>>> conflict 1 of 1 ends\n",
        );
        assert_eq!(texts(&p), (vec!["L\n", "R\n"], vec!["B\n"]));
        assert_eq!(p.labels, labels(&["kx 1a \"left\"", "rr 3c \"right\""], &["ba 2b \"base\""]));
    }

    #[test]
    fn jj_diff_style_applies_the_diff_to_base_and_side() {
        let p = parsed(
            "x\n<<<<<<< conflict 1 of 1\n%%%%%%% diff from: base-label\n\
             \\\\\\\\\\\\\\        to: left-label\n-a\n+left\n ctx\n\n\
             +++++++ right-label\nright\nctx\n\n>>>>>>> conflict 1 of 1 ends\ny\n",
        );
        assert_eq!(
            texts(&p),
            (vec!["x\nleft\nctx\n\ny\n", "x\nright\nctx\n\ny\n"], vec!["x\na\nctx\n\ny\n"])
        );
        assert_eq!(p.labels, labels(&["left-label", "right-label"], &["base-label"]));
    }

    #[test]
    fn jj_diff_style_with_three_sides_keeps_the_side_order() {
        let p = parsed(
            "<<<<<<< conflict 1 of 1\n+++++++ side #1\none\n\
             %%%%%%% diff from: base #1\n\\\\\\\\\\\\\\        to: side #2\n-a\n+two\n\
             %%%%%%% diff from: base #2\n\\\\\\\\\\\\\\        to: side #3\n-b\n+three\n\
             >>>>>>> conflict 1 of 1 ends\n",
        );
        assert_eq!(texts(&p), (vec!["one\n", "two\n", "three\n"], vec!["a\n", "b\n"]));
        assert_eq!(p.labels, labels(&["", "", ""], &["", ""]));
    }

    #[test]
    fn crlf_lines_stay_crlf() {
        let p = parsed("a\r\n<<<<<<< L\r\nl\r\n=======\r\nr\r\n>>>>>>> R\r\nz\r\n");
        assert_eq!(texts(&p), (vec!["a\r\nl\r\nz\r\n", "a\r\nr\r\nz\r\n"], vec!["a\r\nz\r\n"]));
        assert_eq!(p.labels, labels(&["L", "R"], &[""]));
    }

    #[test]
    fn an_end_marker_without_a_newline_takes_one_from_each_section() {
        let p = parsed(
            "<<<<<<< conflict 1 of 1\n+++++++ side #1 (no terminating newline)\nL\n\
             ------- base (no terminating newline)\nB\n+++++++ side #2\nR\n\n\
             >>>>>>> conflict 1 of 1 ends",
        );
        assert_eq!(texts(&p), (vec!["L", "R\n"], vec!["B"]));
        assert_eq!(p.labels, labels(&["", ""], &[""]));
        let crlf = parsed("<<<<<<< a\r\nL\r\n=======\r\nR\r\n>>>>>>> b");
        assert_eq!(texts(&crlf), (vec!["L", "R"], vec![""]));
        // The start marker sets the file's ending: in an LF file a CR before the taken
        // newline is content.
        let lf = parsed("<<<<<<< a\nL\r\n=======\nR\n>>>>>>> b");
        assert_eq!(texts(&lf), (vec!["L\r", "R"], vec![""]));
    }

    #[test]
    fn a_marker_like_line_shorter_than_the_markers_is_content() {
        let p = parsed(
            "<<<<<<<<<<< ours\n<<<<<<< not a marker\n||||||||||| base\nB\n\
             ===========\n=======\n>>>>>>>>>>> theirs\n",
        );
        assert_eq!(texts(&p), (vec!["<<<<<<< not a marker\n", "=======\n"], vec!["B\n"]));
        assert_eq!(p.marker_length, 11);
    }

    #[test]
    fn a_whole_conflict_inside_longer_markers_is_content() {
        let p = parsed(
            "<<<<<<<<<<< ours\n<<<<<<< a\nx\n=======\ny\n>>>>>>> b\n\
             ===========\nR\n>>>>>>>>>>> theirs\n",
        );
        assert_eq!(texts(&p), (vec!["<<<<<<< a\nx\n=======\ny\n>>>>>>> b\n", "R\n"], vec![""]));
        assert_eq!(p.labels, labels(&["ours", "theirs"], &[""]));
        assert_eq!(p.marker_length, 11);
    }

    #[test]
    fn a_longer_run_that_does_not_parse_falls_back_to_the_markers() {
        let p = parsed("<<<<<<<<<< x\n<<<<<<< ours\nL\n=======\nR\n>>>>>>> theirs\n");
        assert_eq!(
            texts(&p),
            (vec!["<<<<<<<<<< x\nL\n", "<<<<<<<<<< x\nR\n"], vec!["<<<<<<<<<< x\n"])
        );
        assert_eq!(p.marker_length, 7);
    }

    #[test]
    fn a_run_followed_by_text_is_no_marker() {
        assert_eq!(error("<<<<<<<x\n=======\n>>>>>>>\n"), MarkerError::NoMarkers);
        assert_eq!(error("<<<<<< six\n"), MarkerError::NoMarkers);
        assert_eq!(error("a\nb\n"), MarkerError::NoMarkers);
        assert_eq!(error(""), MarkerError::NoMarkers);
    }

    #[test]
    fn the_encoding_is_kept() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"<<<<<<<\nL\n=======\nR\n>>>>>>>\n");
        let p = parse(&Text::decode(bytes).expect("text")).expect("parses");
        let encodings: Vec<Encoding> =
            p.inputs.sides().iter().chain(p.inputs.bases()).map(Text::encoding).collect();
        assert_eq!(encodings, [Encoding::Utf8Bom; 3]);
    }

    #[test]
    fn malformed_markers_name_the_line() {
        assert_eq!(error("<<<<<<< x\nL\n=======\nR\n"), MarkerError::Unclosed { line: 1 });
        assert_eq!(
            error("<<<<<<<\nL\n<<<<<<<\nM\n=======\nR\n>>>>>>>\n"),
            MarkerError::Nested { line: 3 }
        );
        assert_eq!(
            error("<<<<<<<\nL\n=======\nR\n|||||||\nB\n>>>>>>>\n"),
            MarkerError::OutOfOrder { line: 5 }
        );
        assert_eq!(
            error("<<<<<<<\nL\n=======\nR\n=======\n>>>>>>>\n"),
            MarkerError::OutOfOrder { line: 5 }
        );
        assert_eq!(
            error("<<<<<<<\nL\n|||||||\nB\n>>>>>>>\n"),
            MarkerError::NoSeparator { line: 5 }
        );
        assert_eq!(
            error("<<<<<<<\n%%%%%%%\nxa\n+++++++\nb\n>>>>>>>\n"),
            MarkerError::BadDiffLine { line: 3 }
        );
        assert_eq!(
            error("<<<<<<<\n+++++++\na\n+++++++\nb\n>>>>>>>\n"),
            MarkerError::Shape { line: 1, sides: 2, bases: 0 }
        );
    }

    #[test]
    fn a_conflict_of_one_side_is_refused() {
        assert_eq!(
            error("a\n<<<<<<<\n+++++++\na\n>>>>>>>\n"),
            MarkerError::Shape { line: 2, sides: 1, bases: 0 }
        );
    }

    #[test]
    fn every_conflict_has_the_same_number_of_sides() {
        assert_eq!(
            error(
                "<<<<<<<\nL\n=======\nR\n>>>>>>>\nm\n\
                 <<<<<<<\n+++++++\na\n-------\nb\n+++++++\nc\n-------\nd\n+++++++\ne\n>>>>>>>\n"
            ),
            MarkerError::MixedArity { line: 7, expected: 2, found: 3 }
        );
    }

    #[test]
    fn errors_name_the_line() {
        assert_eq!(
            MarkerError::Unclosed { line: 4 }.to_string(),
            "line 4: the conflict is not closed"
        );
        assert_eq!(
            MarkerError::Shape { line: 1, sides: 2, bases: 0 }.to_string(),
            "line 1: a conflict needs at least two sides and one base fewer, got 2 sides and 0 bases"
        );
    }
}
