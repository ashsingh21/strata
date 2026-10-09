//! Chords as a guitarist plays them: every shape of a chord that one hand
//! can hold in standard tuning, scored by how easy it is, and for a
//! progression the shapes that keep the hand nearest - fingers that can
//! stay where they are, stay.

use super::voicing::Chord;

/// Standard tuning, low E to high E.
pub const TUNING: [u8; 6] = [40, 45, 50, 55, 59, 64];
pub const STRING_NAMES: [&str; 6] = ["E", "A", "D", "G", "B", "E"];
/// The highest fret a shape may start on.
const TOP_POSITION: u8 = 12;
/// Frets one hand spans without stretching.
const SPAN: u8 = 3;
/// How many of each chord's best shapes the progression search weighs.
const CANDIDATES: usize = 16;

/// Each string's fret, low E first: `None` is not played, `Some(0)` open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    pub frets: [Option<u8>; 6],
}

/// A first finger laid across several strings at one fret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Barre {
    pub fret: u8,
    /// The strings it covers, low to high (indices into `frets`).
    pub from: usize,
    pub to: usize,
}

impl Shape {
    /// The pitches it sounds, low string first.
    pub fn notes(&self) -> Vec<u8> {
        self.frets.iter().zip(TUNING).filter_map(|(f, open)| f.map(|f| open + f)).collect()
    }

    pub fn pitch(&self, string: usize) -> Option<u8> {
        self.frets[string].map(|f| TUNING[string] + f)
    }

    fn fretted(&self) -> impl Iterator<Item = (usize, u8)> + '_ {
        self.frets.iter().enumerate().filter_map(|(s, f)| f.filter(|f| *f > 0).map(|f| (s, f)))
    }

    /// The lowest fret a finger presses (0 for all open strings).
    pub fn low_fret(&self) -> u8 {
        self.fretted().map(|(_, f)| f).min().unwrap_or(0)
    }

    pub fn high_fret(&self) -> u8 {
        self.fretted().map(|(_, f)| f).max().unwrap_or(0)
    }

    /// The barre: the lowest fret pressed on two or more strings with one
    /// finger, every string between them held at that fret or above - when
    /// the shape needs more than four fingers without it, or it's the top
    /// strings (a small barre, like F's last two).
    pub fn barre(&self) -> Option<Barre> {
        let low = self.low_fret();
        if low == 0 {
            return None;
        }
        let at: Vec<usize> = self.fretted().filter(|(_, f)| *f == low).map(|(s, _)| s).collect();
        let (&from, &to) = (at.first()?, at.last()?);
        if at.len() < 2 || !(from..=to).all(|s| self.frets[s].is_some_and(|f| f >= low)) {
            return None;
        }
        (self.fretted().count() > 4 || (to == 5 && at.len() == to - from + 1)).then_some(Barre { fret: low, from, to })
    }

    /// Which finger presses each string (1 = index ... 4 = little),
    /// `None` for open or unplayed. A barre is all finger 1; otherwise
    /// fingers go in order of fret, then string.
    pub fn fingers(&self) -> [Option<u8>; 6] {
        let mut out = [None; 6];
        let barre = self.barre();
        let mut rest: Vec<(usize, u8)> = self.fretted().collect();
        let mut next = 1;
        if let Some(b) = barre {
            for (s, f) in &rest {
                if *f == b.fret {
                    out[*s] = Some(1);
                }
            }
            rest.retain(|(_, f)| *f != b.fret);
            next = 2;
        }
        rest.sort_by_key(|(s, f)| (*f, *s));
        // Two notes on one fret low down take two fingers; the lowest fret
        // gets the first free finger, and the rest follow by distance.
        let base = barre.map(|b| b.fret).unwrap_or_else(|| rest.first().map(|r| r.1).unwrap_or(1));
        for (s, f) in rest {
            let finger = (1 + f - base).max(next).min(4);
            out[s] = Some(finger);
            next = finger + 1;
        }
        out
    }

    /// Fingers it needs.
    fn finger_count(&self) -> usize {
        match self.barre() {
            Some(b) => 1 + self.fretted().filter(|(_, f)| *f != b.fret).count(),
            None => self.fretted().count(),
        }
    }

    /// An open string just above a fretted bass note: a stretch of the
    /// hand that rarely sounds clean (F as 1-0-3-...).
    fn open_over_bass(&self) -> f32 {
        let Some(low) = self.frets.iter().position(|f| f.is_some()) else { return 0.0 };
        let bass_fretted = self.frets[low].is_some_and(|f| f > 0);
        if bass_fretted && low < 5 && self.frets[low + 1] == Some(0) { 1.5 } else { 0.0 }
    }

    /// How hard a top-four-string shape is to hold.
    fn small_effort(&self) -> f32 {
        0.12 * self.low_fret() as f32 + 0.4 * (self.high_fret() - self.low_fret()) as f32 + 0.3 * self.finger_count() as f32
    }

    /// How hard it is to hold: higher is harder.
    fn effort(&self) -> f32 {
        let sounding = self.frets.iter().filter(|f| f.is_some()).count();
        let span = (self.high_fret() - self.low_fret()) as f32;
        0.25 * self.low_fret() as f32
            + 0.4 * span
            + match self.barre() {
                Some(b) if b.to - b.from >= 3 => 1.0,
                Some(_) => 0.2,
                None => 0.0,
            }
            + self.open_over_bass()
            + 0.5 * (6 - sounding) as f32
            + 0.3 * self.finger_count() as f32
    }
}

/// The tones a full shape must have: all of them, but a seventh chord may
/// leave out its fifth.
fn has_tones(notes: &[u8], chord: &Chord) -> bool {
    chord.tones.iter().enumerate().all(|(i, t)| (i == 2 && chord.tones.len() > 3) || notes.iter().any(|n| n % 12 == *t))
}

/// Every shape of `chord` one hand can hold, easiest first: four to six
/// neighbouring strings, the root in the bass, every chord tone (a
/// seventh may drop its fifth), at most four fingers within a reach of
/// four frets.
pub fn shapes(chord: &Chord) -> Vec<Shape> {
    let mut out: Vec<Shape> = Vec::new();
    for position in 1..=TOP_POSITION {
        // Per string: the frets it may take in this position.
        let options: Vec<Vec<Option<u8>>> = (0..6)
            .map(|s| {
                let mut o = vec![None];
                if position <= 4 && chord.tones.contains(&(TUNING[s] % 12)) {
                    o.push(Some(0));
                }
                for f in position..=position + SPAN {
                    if chord.tones.contains(&((TUNING[s] + f) % 12)) {
                        o.push(Some(f));
                    }
                }
                o
            })
            .collect();
        let mut frets = [None; 6];
        collect(&options, 0, &mut frets, chord, &mut out);
    }
    out.sort_by(|a, b| a.effort().total_cmp(&b.effort()).then(a.frets.cmp(&b.frets)));
    out.dedup();
    out
}

fn collect(options: &[Vec<Option<u8>>], string: usize, frets: &mut [Option<u8>; 6], chord: &Chord, out: &mut Vec<Shape>) {
    if string == 6 {
        let shape = Shape { frets: *frets };
        if playable(&shape, chord) && !out.contains(&shape) {
            out.push(shape);
        }
        return;
    }
    for &f in &options[string] {
        frets[string] = f;
        collect(options, string + 1, frets, chord, out);
    }
}

fn playable(shape: &Shape, chord: &Chord) -> bool {
    let sounding: Vec<usize> = (0..6).filter(|&s| shape.frets[s].is_some()).collect();
    let (Some(&first), Some(&last)) = (sounding.first(), sounding.last()) else { return false };
    // Neighbouring strings only: nothing to damp in the middle.
    if sounding.len() < 4 || last - first + 1 != sounding.len() {
        return false;
    }
    let notes = shape.notes();
    if notes[0] % 12 != chord.root || !has_tones(&notes, chord) {
        return false;
    }
    if shape.high_fret() - shape.low_fret() > SPAN || shape.finger_count() > 4 {
        return false;
    }
    // An open string under a barre can't ring.
    if let Some(b) = shape.barre() {
        if (b.from..=b.to).any(|s| shape.frets[s] == Some(0)) {
            return false;
        }
    }
    true
}

/// How far the hand travels from `a` to `b`, less a little for each
/// finger that stays put.
fn travel(a: &Shape, b: &Shape) -> f32 {
    let at = |s: &Shape| if s.low_fret() == 0 { 1.0 } else { s.low_fret() as f32 };
    let held = (0..6).filter(|&i| a.frets[i].is_some_and(|f| f > 0) && a.frets[i] == b.frets[i]).count();
    0.6 * (at(a) - at(b)).abs() - 0.4 * held as f32
}

/// A shape for each chord: easy ones, chosen together so the hand moves
/// as little as it can along the progression.
pub fn lead(chords: &[Chord]) -> Vec<Option<Shape>> {
    let options: Vec<Vec<Shape>> = chords.iter().map(|c| shapes(c).into_iter().take(CANDIDATES).collect()).collect();
    chain(&options, Shape::effort, travel)
}

/// Close shapes on the top four strings (D G B E), one note per string:
/// every chord tone (a seventh may drop its fifth), any of them in the
/// bass, within one hand's reach. Easiest first.
pub fn small_shapes(chord: &Chord) -> Vec<Shape> {
    let mut out: Vec<Shape> = Vec::new();
    for position in 1..=TOP_POSITION {
        let options: Vec<Vec<Option<u8>>> = (0..6)
            .map(|s| {
                if s < 2 {
                    return vec![None];
                }
                let mut o = Vec::new();
                if position <= 4 && chord.tones.contains(&(TUNING[s] % 12)) {
                    o.push(Some(0));
                }
                for f in position..=position + SPAN {
                    if chord.tones.contains(&((TUNING[s] + f) % 12)) {
                        o.push(Some(f));
                    }
                }
                o
            })
            .collect();
        let mut frets = [None; 6];
        collect_small(&options, 0, &mut frets, chord, &mut out);
    }
    out.sort_by(|a, b| a.small_effort().total_cmp(&b.small_effort()).then(a.frets.cmp(&b.frets)));
    out
}

fn collect_small(options: &[Vec<Option<u8>>], string: usize, frets: &mut [Option<u8>; 6], chord: &Chord, out: &mut Vec<Shape>) {
    if string == 6 {
        let shape = Shape { frets: *frets };
        let notes = shape.notes();
        if notes.len() == 4
            && has_tones(&notes, chord)
            && shape.high_fret() - shape.low_fret() <= SPAN
            && shape.finger_count() <= 4
            && !out.contains(&shape)
        {
            out.push(shape);
        }
        return;
    }
    for &f in &options[string] {
        frets[string] = f;
        collect_small(options, string + 1, frets, chord, out);
    }
}

/// Top-four-string shapes for each chord, chosen together so each string's
/// finger moves as few frets as it can - each string is one voice.
pub fn lead_small(chords: &[Chord]) -> Vec<Option<Shape>> {
    let options: Vec<Vec<Shape>> = chords.iter().map(|c| small_shapes(c).into_iter().take(CANDIDATES * 2).collect()).collect();
    chain(&options, Shape::small_effort, |a, b| {
        let frets: u32 = (2..6).map(|s| a.frets[s].unwrap_or(0).abs_diff(b.frets[s].unwrap_or(0)) as u32).sum();
        0.6 * frets as f32
    })
}

/// The cheapest path through each chord's `options`: their own effort plus
/// the travel between neighbours (Viterbi).
fn chain(options: &[Vec<Shape>], effort: impl Fn(&Shape) -> f32, travel: impl Fn(&Shape, &Shape) -> f32) -> Vec<Option<Shape>> {
    let chords = options;
    // Viterbi: the cheapest way to reach each shape of each chord.
    let mut cost: Vec<Vec<(f32, usize)>> = Vec::with_capacity(chords.len());
    for (i, opts) in options.iter().enumerate() {
        let row = opts
            .iter()
            .map(|shape| {
                let own = effort(shape);
                match cost.get(i.wrapping_sub(1)).filter(|_| i > 0) {
                    Some(prev) if !prev.is_empty() => prev
                        .iter()
                        .enumerate()
                        .map(|(j, (c, _))| (c + own + travel(&options[i - 1][j], shape), j))
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .unwrap(),
                    _ => (own, usize::MAX),
                }
            })
            .collect();
        cost.push(row);
    }
    let mut out = vec![None; chords.len()];
    let mut pick = cost.last().and_then(|row| row.iter().enumerate().min_by(|a, b| a.1 .0.total_cmp(&b.1 .0)).map(|(j, _)| j));
    for i in (0..chords.len()).rev() {
        let Some(j) = pick else {
            // A chord with no shape breaks the chain: start again before it.
            pick = cost.get(i.wrapping_sub(1)).and_then(|row| row.iter().enumerate().min_by(|a, b| a.1 .0.total_cmp(&b.1 .0)).map(|(j, _)| j));
            continue;
        };
        out[i] = Some(options[i][j]);
        let back = cost[i][j].1;
        pick = if back == usize::MAX { None } else { Some(back) };
        if pick.is_none() && i > 0 {
            pick = cost[i - 1].iter().enumerate().min_by(|a, b| a.1 .0.total_cmp(&b.1 .0)).map(|(j, _)| j);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theory::voicing::diatonic;

    const C_MAJOR: u16 = 0b1010_1011_0101;

    fn chord(degree: usize, seventh: bool) -> Chord {
        diatonic(degree, 0, C_MAJOR, seventh).unwrap()
    }

    fn tab(s: &Shape) -> String {
        s.frets.iter().map(|f| f.map(|f| if f > 9 { format!("({f})") } else { f.to_string() }).unwrap_or("x".into())).collect()
    }

    #[test]
    fn the_easiest_shapes_are_the_open_chords_guitarists_learn_first() {
        assert_eq!(tab(&shapes(&chord(0, false))[0]), "x32010", "C");
        assert_eq!(tab(&shapes(&chord(4, false))[0]), "320003", "G");
        assert_eq!(tab(&shapes(&chord(5, false))[0]), "x02210", "Am");
        assert_eq!(tab(&shapes(&chord(2, false))[0]), "022000", "Em");
        assert_eq!(tab(&shapes(&chord(1, false))[0]), "xx0231", "Dm");
    }

    #[test]
    fn every_shape_is_the_chord_with_its_root_in_the_bass() {
        for d in 0..7 {
            for seventh in [false, true] {
                let c = chord(d, seventh);
                let all = shapes(&c);
                assert!(!all.is_empty(), "{c:?}");
                for s in all {
                    let notes = s.notes();
                    assert_eq!(notes[0] % 12, c.root);
                    assert!(notes.iter().all(|n| c.tones.contains(&(n % 12))), "{}", tab(&s));
                    assert!(s.high_fret() - s.low_fret() <= SPAN);
                    assert!(s.finger_count() <= 4);
                }
            }
        }
    }

    #[test]
    fn barres_and_fingers() {
        let f = Shape { frets: [Some(1), Some(3), Some(3), Some(2), Some(1), Some(1)] };
        assert_eq!(f.barre(), Some(Barre { fret: 1, from: 0, to: 5 }));
        assert_eq!(f.fingers(), [Some(1), Some(3), Some(4), Some(2), Some(1), Some(1)]);
        let c = Shape { frets: [None, Some(3), Some(2), Some(0), Some(1), Some(0)] };
        assert_eq!(c.barre(), None);
        assert_eq!(c.fingers(), [None, Some(3), Some(2), None, Some(1), None]);
    }

    #[test]
    fn small_shapes_move_a_fret_or_two_per_string() {
        let prog: Vec<Chord> = [5, 3, 0, 4].iter().map(|&d| chord(d, false)).collect();
        let shapes: Vec<Shape> = lead_small(&prog).into_iter().map(|s| s.unwrap()).collect();
        for (s, c) in shapes.iter().zip(&prog) {
            assert_eq!(s.notes().len(), 4);
            assert!(s.frets[0].is_none() && s.frets[1].is_none());
            assert!(c.tones.iter().all(|t| s.notes().iter().any(|n| n % 12 == *t)), "{}", tab(s));
        }
        for pair in shapes.windows(2) {
            let moved: u32 = (2..6).map(|i| pair[0].frets[i].unwrap().abs_diff(pair[1].frets[i].unwrap()) as u32).sum();
            assert!(moved <= 5, "{} -> {}", tab(&pair[0]), tab(&pair[1]));
        }
        println!("{:?}", shapes.iter().map(tab).collect::<Vec<_>>());
    }

    #[test]
    fn a_progression_keeps_the_hand_in_one_place() {
        let prog: Vec<Chord> = [0, 5, 3, 4].iter().map(|&d| chord(d, false)).collect();
        let shapes = lead(&prog);
        assert!(shapes.iter().all(|s| s.is_some()));
        let frets: Vec<u8> = shapes.iter().map(|s| s.unwrap().low_fret()).collect();
        let spread = frets.iter().max().unwrap() - frets.iter().min().unwrap();
        assert!(spread <= 3, "{:?}", shapes.iter().map(|s| tab(&s.unwrap())).collect::<Vec<_>>());
    }
}
