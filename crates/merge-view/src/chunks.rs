//! The top panes' syntax spans, parsed a chunk at a time as the rows show
//! (`docs/PLAN.md` §F4).
//!
//! A row on screen asks for its line with [`ChunkedSpans::want`]. That queues the line's chunk
//! and its neighbours, the focus. [`ChunkedSpans::next_parse`] hands out the queued chunk nearest
//! the focus while fewer than [`Limits::in_flight`] parses run, and
//! [`ChunkedSpans::landed`] keeps its spans. A move of the focus drops the queued chunks
//! left behind; at most [`Limits::cached`] chunks keep their spans, and the one farthest
//! from the focus goes first.

use std::ops::Range;

use merge_syntax::syntax::{LineSpans, Span};

/// Chunks on each side of the focus that parse before they show.
pub const PREFETCH: usize = 1;
/// How far from the focus a queued chunk may be before the queue drops it.
const STALE: usize = PREFETCH + 1;
/// The fewest chunks the cache keeps: the focus and its neighbours.
const MIN_CACHED: usize = 2 * PREFETCH + 1;

/// How much one input's spans may hold and parse at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Chunks whose spans stay; at least the focus and its neighbours.
    pub cached: usize,
    /// Parses running at once; at least one.
    pub in_flight: usize,
}

impl Limits {
    /// 16 chunks of about 2 000 lines, some 5 MB of spans, and two parses at a time.
    pub const DEFAULT: Self = Self { cached: 16, in_flight: 2 };
}

/// A chunk, as long as its lines stay unedited. An edit gives the chunks it touches a new
/// one, so a parse of the lines before the edit lands on no chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    /// Not queued. Its spans, if any, are from before an edit.
    Absent,
    Queued,
    Parsing,
    /// Its spans are those of its lines.
    Parsed,
    /// The parse failed; the chunk shows plain and does not parse again.
    Failed,
}

#[derive(Debug)]
struct Chunk {
    id: ChunkId,
    start: usize,
    status: Status,
    spans: Option<LineSpans>,
}

/// One text's spans, chunk by chunk.
#[derive(Debug)]
pub struct ChunkedSpans {
    /// In order of their lines; chunk n covers its start to the next one's.
    chunks: Vec<Chunk>,
    line_count: usize,
    limits: Limits,
    next_id: u64,
    /// The index of the chunk on screen.
    focus: Option<usize>,
    queued: Vec<ChunkId>,
    flying: Vec<ChunkId>,
    /// The chunks that hold spans, fresh or from before an edit.
    cached: Vec<ChunkId>,
}

impl ChunkedSpans {
    /// The chunks that start at `starts` (`syntax::chunk_starts`) in a text of `line_count`
    /// lines, none parsed.
    #[must_use]
    pub fn new(starts: Vec<usize>, line_count: usize, limits: Limits) -> Self {
        let count = starts.len();
        let chunks = starts
            .into_iter()
            .zip(0..)
            .map(|(start, id)| Chunk {
                id: ChunkId(id),
                start,
                status: Status::Absent,
                spans: None,
            })
            .collect();
        let limits =
            Limits { cached: limits.cached.max(MIN_CACHED), in_flight: limits.in_flight.max(1) };
        Self {
            chunks,
            line_count,
            limits,
            next_id: u64::try_from(count).unwrap_or(u64::MAX),
            focus: None,
            queued: Vec::with_capacity(2 * STALE + 1),
            flying: Vec::with_capacity(limits.in_flight),
            // One over the limit before the cache drops a chunk.
            cached: Vec::with_capacity(limits.cached.min(count).saturating_add(1)),
        }
    }

    fn chunk_of(&self, line: usize) -> Option<usize> {
        if line >= self.line_count {
            return None;
        }
        self.chunks.partition_point(|chunk| chunk.start <= line).checked_sub(1)
    }

    fn index(&self, id: ChunkId) -> Option<usize> {
        self.chunks.iter().position(|chunk| chunk.id == id)
    }

    fn lines(&self, index: usize) -> Range<usize> {
        let end = self.chunks.get(index + 1).map(|next| next.start);
        self.chunks[index].start..end.unwrap_or(self.line_count)
    }

    fn new_id(&mut self) -> ChunkId {
        let id = ChunkId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Line `line`'s spans, in bytes of the line; none for a line past the end or a chunk
    /// whose parse failed. `None` until its chunk has landed. After an edit of its chunk, the
    /// spans from before the edit until the new parse lands.
    #[must_use]
    pub fn line(&self, line: usize) -> Option<&[Span]> {
        let Some(index) = self.chunk_of(line) else {
            return Some(&[]);
        };
        let chunk = &self.chunks[index];
        match (&chunk.spans, chunk.status) {
            (Some(spans), _) => Some(spans.line(line - chunk.start)),
            (None, Status::Failed) => Some(&[]),
            (None, _) => None,
        }
    }

    /// Line `line` shows: its chunk becomes the focus, and it and its neighbours queue
    /// unless they are parsed or parsing.
    pub fn want(&mut self, line: usize) {
        let Some(focus) = self.chunk_of(line) else {
            return;
        };
        if self.focus == Some(focus) {
            return;
        }
        self.focus = Some(focus);
        let chunks = &mut self.chunks;
        self.queued.retain(|&queued| {
            let Some(index) = chunks.iter().position(|chunk| chunk.id == queued) else {
                return false;
            };
            let near = index.abs_diff(focus) <= STALE;
            if !near {
                chunks[index].status = Status::Absent;
            }
            near
        });
        let first = focus.saturating_sub(PREFETCH);
        let near = chunks.iter_mut().skip(first);
        for chunk in near.take(focus + PREFETCH + 1 - first) {
            if chunk.status == Status::Absent {
                chunk.status = Status::Queued;
                self.queued.push(chunk.id);
            }
        }
    }

    /// The queued chunk nearest the focus, the earlier of two as near, and its lines, while
    /// a parse is free. It counts as parsing until it lands.
    pub fn next_parse(&mut self) -> Option<(ChunkId, Range<usize>)> {
        if self.flying.len() >= self.limits.in_flight {
            return None;
        }
        let focus = self.focus?;
        let (at, index) = self
            .queued
            .iter()
            .enumerate()
            .filter_map(|(at, &id)| Some((at, self.index(id)?)))
            .min_by_key(|&(_, index)| (index.abs_diff(focus), index))?;
        self.queued.swap_remove(at);
        let chunk = &mut self.chunks[index];
        chunk.status = Status::Parsing;
        self.flying.push(chunk.id);
        Some((chunk.id, self.lines(index)))
    }

    /// Chunk `id`'s parse has landed with its spans, or failed with none. A chunk that is
    /// not parsing, or that an edit has replaced, takes nothing.
    pub fn landed(&mut self, id: ChunkId, spans: Option<LineSpans>) {
        let Some(at) = self.flying.iter().position(|&flying| flying == id) else {
            return;
        };
        self.flying.swap_remove(at);
        let Some(index) = self.index(id) else {
            return;
        };
        let chunk = &mut self.chunks[index];
        if chunk.status != Status::Parsing {
            return;
        }
        let Some(spans) = spans else {
            chunk.status = Status::Failed;
            chunk.spans = None;
            self.cached.retain(|&cached| cached != id);
            return;
        };
        chunk.status = Status::Parsed;
        if chunk.spans.replace(spans).is_none() {
            self.cached.push(id);
        }
        self.evict(self.focus.unwrap_or(index));
    }

    /// Drop the spans of the chunks farthest from `focus` while more than the limit hold
    /// them.
    fn evict(&mut self, focus: usize) {
        while self.cached.len() > self.limits.cached {
            let farthest = self
                .cached
                .iter()
                .enumerate()
                .filter_map(|(at, &id)| Some((at, self.index(id)?)))
                .max_by_key(|&(_, index)| index.abs_diff(focus));
            let Some((at, index)) = farthest else {
                break;
            };
            self.cached.swap_remove(at);
            let chunk = &mut self.chunks[index];
            chunk.spans = None;
            if chunk.status == Status::Parsed {
                chunk.status = Status::Absent;
            }
        }
    }

    /// Lines `lines` have become `count` lines. The chunks they touch become one, which
    /// queues and becomes the focus when it was queued, parsing or held spans; the chunks after it move
    /// by the lines gained or lost. A chunk alone keeps its spans, those of its edited lines
    /// cleared, until its new parse lands. Returns the chunk the edit is in.
    pub fn edit(&mut self, lines: Range<usize>, count: usize) -> ChunkId {
        let end = lines.end.min(self.line_count);
        let lines = lines.start.min(end)..end;
        let removed = lines.len();
        let id = self.new_id();
        if self.chunks.is_empty() {
            self.chunks.push(Chunk { id, start: 0, status: Status::Absent, spans: None });
        }
        let last_line = self.line_count.saturating_sub(1);
        let first = self.chunk_of(lines.start.min(last_line)).unwrap_or(0);
        let last =
            self.chunk_of(lines.end.saturating_sub(1).min(last_line)).unwrap_or(first).max(first);
        let start = self.chunks[first].start;
        let queue = self.chunks[first..=last].iter().any(|chunk| {
            chunk.spans.is_some() || matches!(chunk.status, Status::Queued | Status::Parsing)
        });
        let touched = &self.chunks[first..=last];
        let gone = |id: &ChunkId| touched.iter().any(|chunk| chunk.id == *id);
        self.queued.retain(|queued| !gone(queued));
        self.cached.retain(|cached| !gone(cached));
        let mut merged = self.chunks.drain(first..=last);
        let spans = match (merged.next(), merged.next()) {
            (Some(Chunk { spans: Some(mut spans), .. }), None) => {
                spans.splice(lines.start - start..lines.end - start, count);
                Some(spans)
            }
            _ => None,
        };
        drop(merged);
        if spans.is_some() {
            self.cached.push(id);
        }
        if queue {
            self.queued.push(id);
        }
        self.chunks.insert(
            first,
            Chunk { id, start, status: if queue { Status::Queued } else { Status::Absent }, spans },
        );
        for chunk in &mut self.chunks[first + 1..] {
            chunk.start = chunk.start + count - removed;
        }
        self.line_count = self.line_count + count - removed;
        // Indices after the edit moved. An edited chunk that queues is on screen.
        self.focus = queue.then_some(first);
        self.evict(first);
        id
    }

    /// When chunk `id` is longer than `max_lines`, it becomes the chunks that start where
    /// `cut` says for its lines, none parsed; `cut` gives their starts in order, the first
    /// the chunk's own.
    pub fn split(
        &mut self,
        id: ChunkId,
        max_lines: usize,
        cut: impl FnOnce(Range<usize>) -> Vec<usize>,
    ) {
        let Some(index) = self.index(id) else {
            return;
        };
        let lines = self.lines(index);
        if lines.len() <= max_lines {
            return;
        }
        let starts = cut(lines.clone());
        if starts.first() != Some(&lines.start) || starts.iter().any(|start| !lines.contains(start))
        {
            tracing::warn!(?lines, ?starts, "a cut does not fit its chunk; the chunk stays whole");
            return;
        }
        self.queued.retain(|&queued| queued != id);
        self.cached.retain(|&cached| cached != id);
        let first = self.next_id;
        self.next_id += u64::try_from(starts.len()).unwrap_or(u64::MAX);
        let parts = starts.into_iter().zip(first..).map(|(start, id)| Chunk {
            id: ChunkId(id),
            start,
            status: Status::Absent,
            spans: None,
        });
        self.chunks.splice(index..=index, parts);
        // Indices moved; the next row on screen sets the focus and queues.
        self.focus = None;
    }

    /// The lines of the text, as edits leave it.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_count
    }

    /// Chunks queued or parsing.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.queued.len() + self.flying.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six chunks of ten lines.
    const LINES: usize = 60;

    fn chunks(limits: Limits) -> ChunkedSpans {
        ChunkedSpans::new(vec![0, 10, 20, 30, 40, 50], LINES, limits)
    }

    /// Ten lines' spans with one span, bytes 1..3, on line `line` of the chunk.
    fn spans(line: usize) -> LineSpans {
        LineSpans::with_span_on(10, line, 1..3)
    }

    #[test]
    fn a_line_has_no_spans_until_its_chunk_lands() {
        let mut chunks = chunks(Limits::DEFAULT);
        assert_eq!(chunks.line(5), None);
        chunks.want(5);
        assert_eq!(chunks.line(5), None, "queued");
        assert_eq!(chunks.next_parse(), Some((ChunkId(0), 0..10)));
        assert_eq!(chunks.line(5), None, "parsing");
        chunks.landed(ChunkId(0), Some(spans(5)));
        assert_eq!(chunks.line(5).map(<[Span]>::len), Some(1));
        assert_eq!(chunks.line(5).map(|line| line[0].range()), Some(1..3));
        assert_eq!(chunks.line(4), Some(&[][..]));
    }

    #[test]
    fn a_landed_chunk_answers_by_line_of_the_text() {
        let mut chunks = chunks(Limits::DEFAULT);
        chunks.want(25);
        assert_eq!(chunks.next_parse(), Some((ChunkId(2), 20..30)));
        chunks.landed(ChunkId(2), Some(spans(3)));
        assert_eq!(chunks.line(23).map(<[Span]>::len), Some(1));
        assert_eq!(chunks.line(20), Some(&[][..]));
        assert_eq!(chunks.line(13), None, "chunk 1 is queued, not landed");
    }

    #[test]
    fn a_line_past_the_end_has_none_and_queues_nothing() {
        let mut chunks = chunks(Limits::DEFAULT);
        assert_eq!(chunks.line(LINES), Some(&[][..]));
        chunks.want(LINES);
        assert_eq!((chunks.next_parse(), chunks.pending()), (None, 0));
        let mut empty = ChunkedSpans::new(Vec::new(), 0, Limits::DEFAULT);
        empty.want(0);
        assert_eq!(empty.next_parse(), None);
        assert_eq!(empty.line(0), Some(&[][..]));
    }

    #[test]
    fn the_focus_parses_first_then_the_nearer_neighbour_and_the_earlier_of_two() {
        let mut chunks = chunks(Limits { cached: 16, in_flight: 3 });
        chunks.want(25);
        assert_eq!(chunks.pending(), 3);
        assert_eq!(chunks.next_parse().map(|(chunk, _)| chunk), Some(ChunkId(2)));
        assert_eq!(chunks.next_parse().map(|(chunk, _)| chunk), Some(ChunkId(1)));
        assert_eq!(chunks.next_parse().map(|(chunk, _)| chunk), Some(ChunkId(3)));
        assert_eq!((chunks.next_parse(), chunks.pending()), (None, 3));
    }

    #[test]
    fn the_first_and_last_chunks_have_one_neighbour() {
        let mut chunks = chunks(Limits::DEFAULT);
        chunks.want(0);
        assert_eq!(chunks.pending(), 2);
        chunks.want(59);
        // Chunks 0 and 1 are four and five away from chunk 5: dropped.
        assert_eq!(chunks.pending(), 2);
        assert_eq!(chunks.next_parse(), Some((ChunkId(5), 50..60)));
        assert_eq!(chunks.next_parse(), Some((ChunkId(4), 40..50)));
    }

    #[test]
    fn no_more_parses_run_than_the_limit_until_one_lands() {
        let mut chunks = chunks(Limits::DEFAULT);
        chunks.want(25);
        assert_eq!(chunks.next_parse().map(|(chunk, _)| chunk), Some(ChunkId(2)));
        assert_eq!(chunks.next_parse().map(|(chunk, _)| chunk), Some(ChunkId(1)));
        assert_eq!(chunks.next_parse(), None);
        chunks.landed(ChunkId(1), Some(spans(0)));
        assert_eq!(chunks.next_parse().map(|(chunk, _)| chunk), Some(ChunkId(3)));
        assert_eq!(chunks.pending(), 2);
    }

    #[test]
    fn a_limit_of_no_parses_still_runs_one() {
        let mut chunks = chunks(Limits { cached: 0, in_flight: 0 });
        chunks.want(5);
        assert_eq!(chunks.next_parse(), Some((ChunkId(0), 0..10)));
        assert_eq!(chunks.next_parse(), None);
    }

    #[test]
    fn a_move_of_the_focus_drops_the_queued_chunks_far_from_it_but_not_the_parsing() {
        let mut chunks = chunks(Limits::DEFAULT);
        chunks.want(5);
        assert_eq!(chunks.next_parse(), Some((ChunkId(0), 0..10)));
        // Queued: 1. Chunk 5 is the focus, 1 is four away and drops; 0 keeps parsing.
        chunks.want(55);
        assert_eq!(chunks.pending(), 3, "0 parsing, 4 and 5 queued");
        assert_eq!(chunks.next_parse(), Some((ChunkId(5), 50..60)));
        chunks.landed(ChunkId(0), Some(spans(5)));
        assert_eq!(chunks.line(5).map(<[Span]>::len), Some(1));
        assert_eq!(chunks.next_parse(), Some((ChunkId(4), 40..50)));
        assert_eq!(chunks.next_parse(), None, "chunk 1 is no longer queued");
        assert_eq!(chunks.line(15), None);
    }

    #[test]
    fn a_queued_chunk_two_from_the_new_focus_stays() {
        let mut chunks = chunks(Limits::DEFAULT);
        chunks.want(25);
        // Queued 1, 2, 3; the focus moves to 3, two from 1.
        chunks.want(35);
        assert_eq!(chunks.pending(), 4);
        let order: Vec<_> = std::iter::from_fn(|| {
            let next = chunks.next_parse().map(|(chunk, _)| chunk);
            if let Some(chunk) = next {
                chunks.landed(chunk, Some(spans(0)));
            }
            next
        })
        .collect();
        assert_eq!(order, [3, 2, 4, 1].map(ChunkId));
    }

    #[test]
    fn the_cache_drops_the_chunk_farthest_from_the_focus() {
        let mut chunks = chunks(Limits { cached: 3, in_flight: 2 });
        chunks.want(5);
        for _ in 0..2 {
            let (chunk, _) = chunks.next_parse().expect("chunks 0 and 1 are queued");
            chunks.landed(chunk, Some(spans(0)));
        }
        chunks.want(55);
        assert_eq!(chunks.next_parse().map(|(chunk, _)| chunk), Some(ChunkId(5)));
        assert_eq!(chunks.next_parse().map(|(chunk, _)| chunk), Some(ChunkId(4)));
        chunks.landed(ChunkId(5), Some(spans(0)));
        assert!(chunks.line(5).is_some(), "three chunks fit");
        chunks.landed(ChunkId(4), Some(spans(0)));
        assert_eq!(chunks.line(5), None, "chunk 0 is five from the focus");
        assert!(chunks.line(15).is_some());
        assert!(chunks.line(45).is_some());
        assert!(chunks.line(55).is_some());
        // A dropped chunk parses again when it shows.
        chunks.want(5);
        assert_eq!(chunks.next_parse(), Some((ChunkId(0), 0..10)));
    }

    #[test]
    fn a_cache_limit_below_the_focus_and_its_neighbours_keeps_them() {
        let mut chunks = chunks(Limits { cached: 1, in_flight: 3 });
        chunks.want(25);
        for _ in 0..3 {
            let (chunk, _) = chunks.next_parse().expect("chunks 1 to 3 are queued");
            chunks.landed(chunk, Some(spans(0)));
        }
        assert!([15, 25, 35].iter().all(|&line| chunks.line(line).is_some()));
    }

    #[test]
    fn a_cache_without_a_limit_keeps_every_chunk() {
        let mut chunks = chunks(Limits { cached: usize::MAX, in_flight: 1 });
        for line in (0..LINES).step_by(10) {
            chunks.want(line);
            while let Some((chunk, _)) = chunks.next_parse() {
                chunks.landed(chunk, Some(spans(0)));
            }
        }
        assert!((0..LINES).all(|line| chunks.line(line).is_some()));
    }

    #[test]
    fn a_failed_chunk_shows_plain_and_does_not_parse_again() {
        let mut chunks = chunks(Limits::DEFAULT);
        chunks.want(5);
        assert_eq!(chunks.next_parse(), Some((ChunkId(0), 0..10)));
        chunks.landed(ChunkId(0), None);
        assert_eq!(chunks.line(5), Some(&[][..]));
        chunks.want(55);
        chunks.want(5);
        assert_eq!(chunks.next_parse(), Some((ChunkId(1), 10..20)));
        assert_eq!(chunks.next_parse(), None);
    }

    #[test]
    fn a_chunk_that_is_not_parsing_takes_nothing() {
        let mut chunks = chunks(Limits::DEFAULT);
        chunks.landed(ChunkId(3), Some(spans(0)));
        chunks.landed(ChunkId(99), Some(spans(0)));
        assert_eq!((chunks.line(35), chunks.pending()), (None, 0));
        chunks.want(5);
        assert_eq!(chunks.next_parse(), Some((ChunkId(0), 0..10)));
        chunks.landed(ChunkId(0), Some(spans(0)));
        // Landing twice does not free a second parse.
        chunks.landed(ChunkId(0), Some(spans(0)));
        assert_eq!(chunks.next_parse(), Some((ChunkId(1), 10..20)));
        assert_eq!(chunks.pending(), 1);
    }

    /// Chunk `chunk` queued, parsed and landed with `spans`.
    fn land(chunks: &mut ChunkedSpans, line: usize, spans: LineSpans) -> ChunkId {
        chunks.want(line);
        let (id, _) = chunks.next_parse().expect("the line's chunk is queued");
        chunks.landed(id, Some(spans));
        id
    }

    #[test]
    fn an_edit_inside_a_chunk_keeps_its_other_spans_and_moves_the_later_chunks() {
        let mut chunks = chunks(Limits::DEFAULT);
        // Chunk 2, lines 20..30, has a span on line 25.
        land(&mut chunks, 25, spans(5));
        // Line 22 becomes three lines: two more.
        let edited = chunks.edit(22..23, 3);
        assert_ne!(edited, ChunkId(2));
        assert_eq!(chunks.line(25), Some(&[][..]), "line 25 is old line 23");
        assert_eq!(chunks.line(27).map(<[Span]>::len), Some(1), "old line 25");
        assert_eq!(chunks.line(22), Some(&[][..]), "edited lines have none");
        // The edited chunk parses again first, over its new lines.
        assert_eq!(chunks.next_parse(), Some((edited, 20..32)));
        assert_eq!(chunks.line(27).map(<[Span]>::len), Some(1), "while it parses");
        chunks.want(32);
        assert_eq!(chunks.next_parse(), Some((ChunkId(3), 32..42)));
        // 62 lines: line 61 is in chunk 5, unparsed; line 62 is past the end.
        assert_eq!(chunks.line(61), None);
        assert_eq!(chunks.line(62), Some(&[][..]));
        chunks.want(61);
        assert_eq!(chunks.pending(), 4, "2 parsing, 4 and 5 queued");
    }

    #[test]
    fn a_parse_that_lands_after_an_edit_of_its_chunk_is_dropped() {
        let mut chunks = chunks(Limits { cached: 16, in_flight: 3 });
        chunks.want(25);
        assert_eq!(chunks.next_parse(), Some((ChunkId(2), 20..30)));
        let edited = chunks.edit(22..23, 1);
        chunks.landed(ChunkId(2), Some(spans(5)));
        assert_eq!(chunks.line(25), None, "the parse was of the old lines");
        assert_eq!(chunks.next_parse(), Some((edited, 20..30)));
        chunks.landed(edited, Some(spans(5)));
        assert_eq!(chunks.line(25).map(<[Span]>::len), Some(1));
    }

    #[test]
    fn a_parse_in_flight_holds_its_slot_until_it_lands_after_an_edit() {
        let mut chunks = chunks(Limits { cached: 16, in_flight: 1 });
        chunks.want(25);
        assert_eq!(chunks.next_parse(), Some((ChunkId(2), 20..30)));
        let edited = chunks.edit(22..23, 1);
        assert_eq!(chunks.next_parse(), None);
        // 1, 3 and the edited chunk queued; the old parse of 2 flying.
        assert_eq!(chunks.pending(), 4);
        chunks.landed(ChunkId(2), Some(spans(0)));
        assert_eq!(chunks.next_parse(), Some((edited, 20..30)));
    }

    #[test]
    fn an_edit_across_chunks_merges_them_without_spans() {
        let mut chunks = chunks(Limits { cached: 16, in_flight: 3 });
        land(&mut chunks, 15, spans(0));
        chunks.want(25);
        let (id, _) = chunks.next_parse().expect("chunk 2 is queued");
        chunks.landed(id, Some(spans(0)));
        // Lines 15..25 become one: chunks 1 (10..20) and 2 (20..30) become 10..21.
        let edited = chunks.edit(15..25, 1);
        assert_eq!(chunks.line(12), None);
        // 51 lines: line 50 is in the last chunk, unparsed; line 51 is past the end.
        assert_eq!(chunks.line(50), None);
        assert_eq!(chunks.line(51), Some(&[][..]));
        assert_eq!(
            chunks.next_parse().map(|(id, lines)| (id == edited, lines)),
            Some((true, 10..21))
        );
        chunks.want(21);
        assert_eq!(chunks.next_parse(), Some((ChunkId(3), 21..31)));
    }

    #[test]
    fn an_edit_of_a_chunk_that_never_showed_queues_nothing() {
        let mut chunks = chunks(Limits::DEFAULT);
        chunks.edit(44..45, 2);
        assert_eq!((chunks.pending(), chunks.next_parse()), (0, None));
        assert_eq!(chunks.line(44), None);
        chunks.want(55);
        assert_eq!(chunks.next_parse(), Some((ChunkId(5), 51..61)));
    }

    #[test]
    fn a_long_chunk_splits_where_the_cut_says() {
        let mut chunks = chunks(Limits::DEFAULT);
        land(&mut chunks, 5, spans(0));
        // Line 5 becomes 30 lines: chunk 0 covers 0..39.
        let edited = chunks.edit(5..6, 30);
        chunks.split(edited, 20, |lines| {
            assert_eq!(lines, 0..39);
            vec![0, 15, 30]
        });
        assert_eq!(chunks.line(5), None, "the parts have no spans");
        chunks.want(16);
        assert_eq!(chunks.next_parse().map(|(_, lines)| lines), Some(15..30));
        assert_eq!(chunks.next_parse().map(|(_, lines)| lines), Some(0..15));
        chunks.want(39);
        assert_eq!(chunks.next_parse(), None, "two parses run");
        // 30..39, 39..49 and 49..59 queued; 15..30 and 0..15 parsing.
        assert_eq!(chunks.pending(), 5);
    }

    #[test]
    fn a_chunk_within_the_limit_or_a_bad_cut_stays_whole() {
        let mut chunks = chunks(Limits::DEFAULT);
        let edited = chunks.edit(5..6, 2);
        chunks.split(edited, 20, |_| unreachable!("11 lines fit"));
        let edited = chunks.edit(5..6, 30);
        chunks.split(edited, 20, |_| vec![1, 15]);
        chunks.split(edited, 20, |_| vec![0, 50]);
        chunks.want(5);
        assert_eq!(chunks.next_parse(), Some((edited, 0..40)));
    }

    #[test]
    fn an_edit_of_a_text_of_one_line_grows_its_chunk() {
        let mut chunks = ChunkedSpans::new(vec![0], 1, Limits::DEFAULT);
        chunks.edit(0..1, 5);
        chunks.want(4);
        assert_eq!(chunks.next_parse().map(|(_, lines)| lines), Some(0..5));
        let mut empty = ChunkedSpans::new(Vec::new(), 0, Limits::DEFAULT);
        empty.edit(0..0, 3);
        empty.want(2);
        assert_eq!(empty.next_parse().map(|(_, lines)| lines), Some(0..3));
    }
}
