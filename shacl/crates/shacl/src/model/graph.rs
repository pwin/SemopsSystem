//! An immutable, indexed RDF graph.
//!
//! Triples are held in three fully-sorted permutations rather than in hash maps
//! of adjacency lists. Every lookup SHACL needs is a prefix range on one of
//! them, walked as a contiguous slice — which keeps path evaluation, the
//! engine's hottest loop, free of both pointer chasing and allocation.
//!
//! Each permutation carries a first-level index: for every term that appears
//! in its leading position, the offset of that term's run of rows. Term ids are
//! dense integers, so this is a direct array load rather than a search. It
//! replaced a binary search over the whole permutation, and the difference is
//! not the arithmetic: on 690k triples the rows are 25 MB per permutation,
//! the deep probes of a search miss cache, and a validation run does ten of
//! them per focus node. Measured on that data, single-threaded, the validate
//! phase went from 0.170s to 0.134s — a fifth, not the half predicted, because
//! the term and string arrays are missed the same way and this fixes only the
//! rows. It is the first level of the trie the HOLOS store builds over the
//! same idea.

use super::term::TermId;

/// A triple of interned terms, stored in whatever component order its index
/// implies.
type Row = [TermId; 3];

/// Accumulates triples before they are sorted into a [`Graph`].
#[derive(Debug, Default)]
pub struct GraphBuilder {
    rows: Vec<Row>,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, s: TermId, p: TermId, o: TermId) {
        self.rows.push([s, p, o]);
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Sorts and deduplicates into the three index permutations.
    pub fn build(mut self) -> Graph {
        self.rows.sort_unstable();
        self.rows.dedup();

        let spo = self.rows;
        let mut pos: Vec<Row> = spo.iter().map(|&[s, p, o]| [p, o, s]).collect();
        pos.sort_unstable();
        let mut osp: Vec<Row> = spo.iter().map(|&[s, p, o]| [o, s, p]).collect();
        osp.sort_unstable();

        let spo_starts = Starts::build(&spo);
        let pos_starts = Starts::build(&pos);
        let osp_starts = Starts::build(&osp);
        Graph {
            spo,
            pos,
            osp,
            spo_starts,
            pos_starts,
            osp_starts,
        }
    }
}

/// The first-level index over one sorted permutation.
///
/// `starts[a - base] .. starts[a - base + 1]` is the run of rows whose leading
/// component is `a`. Offsets are `u32` because rows are, and it is indexed by
/// term id, so a lookup is one load — the point of the structure.
///
/// `base` is the smallest leading id, so the array spans the ids the graph
/// actually uses rather than the whole store. That matters for a small graph
/// interned after a large one: the shapes graph, or every graph the rules
/// engine hands back, would otherwise carry an index the size of the data
/// graph's id space for a few hundred triples.
#[derive(Debug, Default, Clone)]
struct Starts {
    base: u32,
    starts: Vec<u32>,
}

impl Starts {
    fn build(rows: &[Row]) -> Self {
        let Some(first) = rows.first() else {
            return Self::default();
        };
        assert!(
            rows.len() < u32::MAX as usize,
            "a graph is indexed by u32 row offsets"
        );
        // Sorted, so the ends of the slice bound the leading component.
        let base = first[0].0;
        let last = rows[rows.len() - 1][0].0;
        let span = (last - base) as usize + 1;
        // One counting pass and one prefix sum: `starts[i + 1]` accumulates
        // the rows led by id `base + i`, so after the sum `starts[i]` is where
        // that id's run begins and `starts[i + 1]` where it ends.
        let mut starts = vec![0u32; span + 1];
        for r in rows {
            starts[(r[0].0 - base) as usize + 1] += 1;
        }
        for i in 1..starts.len() {
            starts[i] += starts[i - 1];
        }
        Self { base, starts }
    }

    /// Records one row led by `a` inserted into the permutation, or removed
    /// from it: the runs after `a`'s move by one.
    ///
    /// A leading id outside the span grows the array at that end with empty
    /// runs; one inside it costs the tail after `a`, as the row's memmove did.
    /// Removal never shrinks it, because an empty run at either end answers a
    /// lookup exactly as no entry does.
    fn shift(&mut self, a: TermId, inserted: bool) {
        let a = a.0;
        if self.starts.is_empty() {
            self.base = a;
            self.starts = vec![0, 0];
        } else if a < self.base {
            // Every existing run starts at or after offset 0, so the new runs
            // in front of them are empty.
            let grow = (self.base - a) as usize;
            self.starts.splice(0..0, std::iter::repeat_n(0, grow));
            self.base = a;
        } else if (a - self.base) as usize + 1 >= self.starts.len() {
            let end = self.starts[self.starts.len() - 1];
            self.starts.resize((a - self.base) as usize + 2, end);
        }
        let i = (a - self.base) as usize;
        for start in &mut self.starts[i + 1..] {
            if inserted {
                *start += 1;
            } else {
                *start -= 1;
            }
        }
    }

    /// The rows led by `a`, or none.
    #[inline]
    fn run<'r>(&self, rows: &'r [Row], a: TermId) -> &'r [Row] {
        let Some(i) = a.0.checked_sub(self.base).map(|i| i as usize) else {
            return &[];
        };
        if i + 1 >= self.starts.len() {
            return &[];
        }
        &rows[self.starts[i] as usize..self.starts[i + 1] as usize]
    }
}

/// An immutable RDF graph indexed for the access patterns SHACL uses.
///
/// `Clone` copies all three permutations — three words per triple — rather
/// than rebuilding and re-sorting them, which is what the rules engine wants
/// when it hands back a graph nothing was added to.
#[derive(Debug, Default, Clone)]
pub struct Graph {
    /// Sorted by `(subject, predicate, object)`.
    spo: Vec<Row>,
    /// Sorted by `(predicate, object, subject)`.
    pos: Vec<Row>,
    /// Sorted by `(object, subject, predicate)`.
    osp: Vec<Row>,
    /// Where each subject's rows begin in `spo`.
    spo_starts: Starts,
    /// Where each predicate's rows begin in `pos`.
    pos_starts: Starts,
    /// Where each object's rows begin in `osp`.
    osp_starts: Starts,
}

impl Graph {
    /// Applies a batch of additions and removals, for a caller that keeps one
    /// graph current as its source changes: HOLOS's incremental revalidation.
    ///
    /// Removals are applied first, so a delta that removes and re-adds the same
    /// triple ends with it present — which is what a store reporting a value's
    /// replacement produces, and the other order would silently drop it.
    ///
    /// A small delta edits each sorted permutation in place and recounts its
    /// first-level index in one pass; a large one rebuilds the graph. The
    /// crossover is where `d` memmoves of `n` rows stop being cheaper than one
    /// sort of `n + d`, and it is not delicate: a holon tick carries a handful
    /// of triples and a bulk load all of them.
    pub fn apply(&mut self, added: &[Row], removed: &[Row]) {
        const REBUILD_ABOVE: usize = 64;

        if added.len() + removed.len() > REBUILD_ABOVE.max(self.len() / 16) {
            let dropped: std::collections::HashSet<Row> = removed.iter().copied().collect();
            let mut rows: Vec<Row> = Vec::with_capacity(self.spo.len() + added.len());
            rows.extend(self.spo.iter().copied().filter(|r| !dropped.contains(r)));
            rows.extend_from_slice(added);
            *self = GraphBuilder { rows }.build();
            return;
        }

        // One binary search and one memmove per row per permutation, and its
        // first-level index shifted from the row's leading id on: both touch
        // only the tail after the row, which for new terms (the highest ids)
        // is nearly nothing.
        let edit = |rows: &mut Vec<Row>, starts: &mut Starts, key: fn(Row) -> Row| {
            for &r in removed {
                let k = key(r);
                if let Ok(i) = rows.binary_search(&k) {
                    rows.remove(i);
                    starts.shift(k[0], false);
                }
            }
            for &r in added {
                let k = key(r);
                if let Err(i) = rows.binary_search(&k) {
                    rows.insert(i, k);
                    starts.shift(k[0], true);
                }
            }
        };
        edit(&mut self.spo, &mut self.spo_starts, |r| r);
        edit(&mut self.pos, &mut self.pos_starts, |[s, p, o]| [p, o, s]);
        edit(&mut self.osp, &mut self.osp_starts, |[s, p, o]| [o, s, p]);
    }

    pub fn len(&self) -> usize {
        self.spo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spo.is_empty()
    }

    /// Objects of `(s, p, ?)` — the predicate-path hot path.
    #[inline]
    pub fn objects(&self, s: TermId, p: TermId) -> impl Iterator<Item = TermId> + '_ {
        prefix2(self.spo_starts.run(&self.spo, s), p)
            .iter()
            .map(|r| r[2])
    }

    /// The first object of `(s, p, ?)`, for functional properties.
    #[inline]
    pub fn object(&self, s: TermId, p: TermId) -> Option<TermId> {
        prefix2(self.spo_starts.run(&self.spo, s), p)
            .first()
            .map(|r| r[2])
    }

    /// Subjects of `(?, p, o)` — inverse paths and `sh:targetSubjectsOf`.
    #[inline]
    pub fn subjects(&self, p: TermId, o: TermId) -> impl Iterator<Item = TermId> + '_ {
        prefix2(self.pos_starts.run(&self.pos, p), o)
            .iter()
            .map(|r| r[2])
    }

    /// Every subject appearing with predicate `p`, in sorted order with
    /// duplicates retained.
    #[inline]
    pub fn subjects_of(&self, p: TermId) -> impl Iterator<Item = TermId> + '_ {
        self.pos_starts.run(&self.pos, p).iter().map(|r| r[2])
    }

    /// Every object appearing with predicate `p`.
    #[inline]
    pub fn objects_of(&self, p: TermId) -> impl Iterator<Item = TermId> + '_ {
        self.pos_starts.run(&self.pos, p).iter().map(|r| r[1])
    }

    /// All `(predicate, object)` pairs of `s` — used by `sh:closed`.
    #[inline]
    pub fn predicate_objects(&self, s: TermId) -> impl Iterator<Item = (TermId, TermId)> + '_ {
        self.spo_starts
            .run(&self.spo, s)
            .iter()
            .map(|r| (r[1], r[2]))
    }

    /// All `(subject, predicate)` pairs pointing at `o`.
    #[inline]
    pub fn subject_predicates(&self, o: TermId) -> impl Iterator<Item = (TermId, TermId)> + '_ {
        self.osp_starts
            .run(&self.osp, o)
            .iter()
            .map(|r| (r[1], r[2]))
    }

    #[inline]
    pub fn contains(&self, s: TermId, p: TermId, o: TermId) -> bool {
        self.spo_starts
            .run(&self.spo, s)
            .binary_search(&[s, p, o])
            .is_ok()
    }

    /// True if `s` appears as a subject of any triple.
    #[inline]
    pub fn has_subject(&self, s: TermId) -> bool {
        !self.spo_starts.run(&self.spo, s).is_empty()
    }

    /// Every triple, in `(s, p, o)` order.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = Row> + '_ {
        self.spo.iter().copied()
    }
}

/// Within `run` — the rows led by one term, from [`Starts::run`] — the
/// contiguous slice whose second component equals `b`.
///
/// Still a binary search, but over one term's rows rather than the graph's: a
/// subject has seven triples on the benchmark data, not 690,000, so this stays
/// in the cache lines the first-level load already brought in.
#[inline]
fn prefix2(run: &[Row], b: TermId) -> &[Row] {
    let lo = run.partition_point(|r| r[1] < b);
    let hi = run[lo..].partition_point(|r| r[1] == b) + lo;
    &run[lo..hi]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(n: u32) -> TermId {
        TermId(n)
    }

    /// `:a :p :b`, `:a :p :c`, `:a :q :b`, `:d :p :b`
    fn sample() -> Graph {
        let mut b = GraphBuilder::new();
        b.push(t(0), t(1), t(2));
        b.push(t(0), t(1), t(3));
        b.push(t(0), t(4), t(2));
        b.push(t(5), t(1), t(2));
        b.push(t(0), t(1), t(2)); // duplicate
        b.build()
    }

    #[test]
    fn deduplicates_on_build() {
        assert_eq!(sample().len(), 4);
    }

    #[test]
    fn objects_selects_by_subject_and_predicate() {
        let g = sample();
        let got: Vec<_> = g.objects(t(0), t(1)).collect();
        assert_eq!(got, vec![t(2), t(3)]);
        assert_eq!(g.objects(t(0), t(4)).collect::<Vec<_>>(), vec![t(2)]);
        assert_eq!(g.objects(t(9), t(1)).count(), 0, "absent subject");
    }

    #[test]
    fn subjects_selects_by_predicate_and_object() {
        let g = sample();
        let got: Vec<_> = g.subjects(t(1), t(2)).collect();
        assert_eq!(got, vec![t(0), t(5)]);
        assert_eq!(g.subjects(t(1), t(9)).count(), 0);
    }

    #[test]
    fn predicate_and_object_projections() {
        let g = sample();
        // `pos` is ordered by (predicate, object, subject), so subjects come
        // back grouped by object rather than sorted.
        assert_eq!(
            g.subjects_of(t(1)).collect::<Vec<_>>(),
            vec![t(0), t(5), t(0)]
        );
        assert_eq!(
            g.objects_of(t(1)).collect::<Vec<_>>(),
            vec![t(2), t(2), t(3)]
        );
        assert_eq!(g.objects_of(t(99)).count(), 0);
    }

    #[test]
    fn adjacency_views() {
        let g = sample();
        let mut po: Vec<_> = g.predicate_objects(t(0)).collect();
        po.sort();
        assert_eq!(po, vec![(t(1), t(2)), (t(1), t(3)), (t(4), t(2))]);

        let mut sp: Vec<_> = g.subject_predicates(t(3)).collect();
        sp.sort();
        assert_eq!(sp, vec![(t(0), t(1))]);
    }

    #[test]
    fn contains_and_has_subject() {
        let g = sample();
        assert!(g.contains(t(0), t(1), t(2)));
        assert!(!g.contains(t(0), t(1), t(9)));
        assert!(g.has_subject(t(5)));
        assert!(!g.has_subject(t(2)), "only ever an object");
    }

    #[test]
    fn empty_graph_answers_everything_emptily() {
        let g = GraphBuilder::new().build();
        assert!(g.is_empty());
        assert_eq!(g.objects(t(0), t(1)).count(), 0);
        assert!(!g.contains(t(0), t(1), t(2)));
    }

    // ----------------------------------------------------------------- deltas

    /// Every triple in the graph, read back through each of the three indexes separately.
    ///
    /// A mutation that updates `spo` and forgets `pos` leaves a graph that answers one
    /// question correctly and another wrongly, and every caller-level test would still pass
    /// as long as it happened to ask the first. So the check is per index, not per graph.
    fn dump(g: &Graph) -> (Vec<Row>, Vec<Row>, Vec<Row>) {
        let mut by_spo: Vec<Row> = g.iter().collect();
        let mut by_pos: Vec<Row> = Vec::new();
        let mut by_osp: Vec<Row> = Vec::new();
        // `subjects_of` walks `pos`; `subject_predicates` walks `osp`.
        let predicates: std::collections::BTreeSet<TermId> = by_spo.iter().map(|r| r[1]).collect();
        for p in predicates {
            for sub in g.subjects_of(p) {
                for o in g.objects(sub, p) {
                    by_pos.push([sub, p, o]);
                }
            }
        }
        let objects: std::collections::BTreeSet<TermId> = by_spo.iter().map(|r| r[2]).collect();
        for o in objects {
            for (sub, p) in g.subject_predicates(o) {
                by_osp.push([sub, p, o]);
            }
        }
        by_spo.sort_unstable();
        by_pos.sort_unstable();
        by_pos.dedup();
        by_osp.sort_unstable();
        by_osp.dedup();
        (by_spo, by_pos, by_osp)
    }

    fn rebuilt(rows: &[Row]) -> Graph {
        let mut b = GraphBuilder::new();
        for &[s, p, o] in rows {
            b.push(s, p, o);
        }
        b.build()
    }

    /// A deterministic pseudo-random sequence, so a failure is reproducible.
    fn lcg(seed: &mut u64) -> u32 {
        *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (*seed >> 33) as u32
    }

    #[test]
    fn a_delta_that_removes_and_re_adds_ends_with_the_triple_present() {
        // A store reporting a value's replacement produces exactly this, and applying the
        // additions first would drop it.
        let mut g = sample();
        g.apply(&[[t(0), t(1), t(2)]], &[[t(0), t(1), t(2)]]);
        assert!(g.contains(t(0), t(1), t(2)));
    }

    /// The differential property: however a graph is reached, it answers like one built from
    /// the same triples in one go.
    ///
    /// Run either side of the rebuild threshold, because the two strategies are separate
    /// code and a test that only exercised one would leave the other unwitnessed. The
    /// threshold itself is a performance choice and no correctness test can pin it: moving it
    /// changes which branch runs and nothing else, which is exactly the property these assert.
    fn agrees_with_a_rebuild(batch: usize, rounds: usize, seed: u64) {
        let mut seed = seed;
        let mut g = Graph::default();
        let mut model: std::collections::BTreeSet<Row> = std::collections::BTreeSet::new();

        for _ in 0..rounds {
            let mut added = Vec::new();
            let mut removed = Vec::new();
            for _ in 0..batch {
                let row = [
                    t(lcg(&mut seed) % 12),
                    t(lcg(&mut seed) % 4),
                    t(lcg(&mut seed) % 12),
                ];
                if lcg(&mut seed).is_multiple_of(3) {
                    removed.push(row);
                } else {
                    added.push(row);
                }
            }
            // The model applies them in the same order the graph promises to.
            for r in &removed {
                model.remove(r);
            }
            for r in &added {
                model.insert(*r);
            }
            g.apply(&added, &removed);

            let expected: Vec<Row> = model.iter().copied().collect();
            assert_eq!(
                dump(&g),
                dump(&rebuilt(&expected)),
                "batch={batch} after applying {} additions and {} removals",
                added.len(),
                removed.len()
            );
            assert_eq!(g.len(), expected.len());
        }
    }

    /// A delta bringing ids below and above every id the graph held grows the
    /// first-level index at both ends, and removing them leaves empty runs that
    /// answer like no entry.
    #[test]
    fn a_delta_beyond_either_end_of_the_id_span_is_indexed() {
        let mut g = rebuilt(&[[t(5), t(6), t(7)], [t(6), t(6), t(5)]]);
        let low = [t(1), t(2), t(3)];
        let high = [t(40), t(41), t(42)];
        g.apply(&[low, high], &[]);
        assert_eq!(
            dump(&g),
            dump(&rebuilt(&[
                [t(5), t(6), t(7)],
                [t(6), t(6), t(5)],
                low,
                high
            ]))
        );
        g.apply(&[], &[low, high, [t(5), t(6), t(7)]]);
        assert_eq!(dump(&g), dump(&rebuilt(&[[t(6), t(6), t(5)]])));
        assert_eq!(g.objects(t(1), t(2)).count(), 0);
        assert_eq!(g.objects(t(40), t(41)).count(), 0);
        g.apply(&[], &[[t(6), t(6), t(5)]]);
        assert!(g.is_empty());
        g.apply(&[high], &[]);
        assert_eq!(dump(&g), dump(&rebuilt(&[high])));
    }

    #[test]
    fn small_deltas_agree_with_a_rebuild() {
        agrees_with_a_rebuild(4, 60, 0x5eed);
    }

    #[test]
    fn large_deltas_agree_with_a_rebuild() {
        // Past `REBUILD_ABOVE`, so this exercises the other branch.
        agrees_with_a_rebuild(200, 12, 0xc0ffee);
    }
}
