//! Famous classical melodies (all long out of copyright), cut into
//! phrases to practise by ear: hear a phrase, play it back, then the next.
//! Each note is (16th from the phrase's start, pitch, length in 16ths);
//! middle C is 60. Keys are the usual ones (or the usual teaching key).

use crate::practice::{Exercise, Kind};

pub struct Piece {
    pub title: &'static str,
    pub composer: &'static str,
    /// A comfortable practice tempo (slower than concert pace).
    pub bpm: i32,
    pub phrases: &'static [&'static [(i64, u8, i64)]],
}

impl Piece {
    /// Phrase `i` as an exercise (a melody, one or two bars).
    pub fn exercise(&self, i: usize) -> Exercise {
        let notes = self.phrases[i % self.phrases.len()].to_vec();
        let end = notes.iter().map(|n| n.0 + n.2).max().unwrap_or(16);
        Exercise { kind: Kind::Melody, level: 0, notes, bars: ((end + 15) / 16).max(1) }
    }
}

// Ode to Joy: the theme of the last movement of the Ninth Symphony.
const ODE_A: &[(i64, u8, i64)] = &[(0, 64, 4), (4, 64, 4), (8, 65, 4), (12, 67, 4), (16, 67, 4), (20, 65, 4), (24, 64, 4), (28, 62, 4)];
const ODE_B: &[(i64, u8, i64)] = &[(0, 60, 4), (4, 60, 4), (8, 62, 4), (12, 64, 4), (16, 64, 6), (22, 62, 2), (24, 62, 8)];
const ODE_C: &[(i64, u8, i64)] = &[(0, 60, 4), (4, 60, 4), (8, 62, 4), (12, 64, 4), (16, 62, 6), (22, 60, 2), (24, 60, 8)];
const ODE_D: &[(i64, u8, i64)] = &[(0, 62, 4), (4, 62, 4), (8, 64, 4), (12, 60, 4), (16, 62, 4), (20, 64, 2), (22, 65, 2), (24, 64, 4), (28, 60, 4)];
const ODE_E: &[(i64, u8, i64)] = &[(0, 62, 4), (4, 64, 2), (6, 65, 2), (8, 64, 4), (12, 62, 4), (16, 60, 4), (20, 62, 4), (24, 55, 8)];

// Für Elise: the right hand's opening, both endings.
const ELISE_A: &[(i64, u8, i64)] = &[
    (0, 76, 1), (1, 75, 1), (2, 76, 1), (3, 75, 1), (4, 76, 1), (5, 71, 1), (6, 74, 1), (7, 72, 1), (8, 69, 2),
    (11, 60, 1), (12, 64, 1), (13, 69, 1), (14, 71, 2), (17, 64, 1), (18, 68, 1), (19, 71, 1), (20, 72, 2),
];
const ELISE_B: &[(i64, u8, i64)] = &[
    (0, 64, 1), (1, 76, 1), (2, 75, 1), (3, 76, 1), (4, 75, 1), (5, 76, 1), (6, 71, 1), (7, 74, 1), (8, 72, 1), (9, 69, 2),
    (12, 60, 1), (13, 64, 1), (14, 69, 1), (15, 71, 2), (18, 64, 1), (19, 72, 1), (20, 71, 1), (21, 69, 4),
];

// Eine kleine Nachtmusik: the opening call and its answer.
const NACHT_A: &[(i64, u8, i64)] = &[(0, 67, 4), (6, 62, 2), (8, 67, 4), (14, 62, 2), (16, 67, 2), (18, 62, 2), (20, 67, 2), (22, 71, 2), (24, 74, 4)];
const NACHT_B: &[(i64, u8, i64)] = &[(0, 72, 4), (6, 69, 2), (8, 72, 4), (14, 69, 2), (16, 72, 2), (18, 69, 2), (20, 66, 2), (22, 69, 2), (24, 62, 4)];

// Minuet in G (from the Anna Magdalena notebook, by Petzold): 3/4, two
// bars to a phrase.
const MINUET_A: &[(i64, u8, i64)] = &[(0, 74, 4), (4, 67, 2), (6, 69, 2), (8, 71, 2), (10, 72, 2), (12, 74, 4), (16, 67, 4), (20, 67, 4)];
const MINUET_B: &[(i64, u8, i64)] = &[(0, 76, 4), (4, 72, 2), (6, 74, 2), (8, 76, 2), (10, 78, 2), (12, 79, 4), (16, 67, 4), (20, 67, 4)];
const MINUET_C: &[(i64, u8, i64)] = &[(0, 72, 4), (4, 74, 2), (6, 72, 2), (8, 71, 2), (10, 69, 2), (12, 71, 4), (16, 72, 2), (18, 71, 2), (20, 69, 2), (22, 67, 2)];
const MINUET_D: &[(i64, u8, i64)] = &[(0, 66, 4), (4, 67, 2), (6, 69, 2), (8, 71, 2), (10, 67, 2), (12, 69, 12)];

// In the Hall of the Mountain King, in A minor.
const KING_A: &[(i64, u8, i64)] = &[
    (0, 57, 2), (2, 59, 2), (4, 60, 2), (6, 62, 2), (8, 64, 2), (10, 60, 2), (12, 64, 4),
    (16, 63, 2), (18, 59, 2), (20, 63, 4), (24, 62, 2), (26, 58, 2), (28, 62, 4),
];
const KING_B: &[(i64, u8, i64)] = &[
    (0, 57, 2), (2, 59, 2), (4, 60, 2), (6, 62, 2), (8, 64, 2), (10, 60, 2), (12, 64, 2), (14, 69, 2),
    (16, 67, 2), (18, 64, 2), (20, 60, 2), (22, 64, 2), (24, 67, 8),
];

// Twinkle, Twinkle: the tune of Mozart's variations "Ah vous dirai-je, maman".
const TWINKLE_A: &[(i64, u8, i64)] = &[(0, 60, 4), (4, 60, 4), (8, 67, 4), (12, 67, 4), (16, 69, 4), (20, 69, 4), (24, 67, 8)];
const TWINKLE_B: &[(i64, u8, i64)] = &[(0, 65, 4), (4, 65, 4), (8, 64, 4), (12, 64, 4), (16, 62, 4), (20, 62, 4), (24, 60, 8)];
const TWINKLE_C: &[(i64, u8, i64)] = &[(0, 67, 4), (4, 67, 4), (8, 65, 4), (12, 65, 4), (16, 64, 4), (20, 64, 4), (24, 62, 8)];

// Canon in D: the first violin's opening lines, in half notes.
const CANON_A: &[(i64, u8, i64)] = &[(0, 78, 8), (8, 76, 8), (16, 74, 8), (24, 73, 8)];
const CANON_B: &[(i64, u8, i64)] = &[(0, 71, 8), (8, 69, 8), (16, 71, 8), (24, 73, 8)];
const CANON_C: &[(i64, u8, i64)] = &[(0, 74, 8), (8, 73, 8), (16, 71, 8), (24, 69, 8)];
const CANON_D: &[(i64, u8, i64)] = &[(0, 67, 8), (8, 66, 8), (16, 67, 8), (24, 64, 8)];

// The Fifth Symphony's knock: three short notes and a long one, twice.
const FIFTH_A: &[(i64, u8, i64)] = &[(2, 67, 2), (4, 67, 2), (6, 67, 2), (8, 63, 8), (18, 65, 2), (20, 65, 2), (22, 65, 2), (24, 62, 8)];

// Symphony No. 40: the sighing opening theme.
const FORTY_A: &[(i64, u8, i64)] = &[(0, 75, 2), (2, 74, 2), (4, 74, 4), (8, 75, 2), (10, 74, 2), (12, 74, 4), (16, 75, 2), (18, 74, 2), (20, 74, 4), (24, 82, 8)];
const FORTY_B: &[(i64, u8, i64)] = &[(0, 82, 2), (2, 81, 2), (4, 79, 4), (8, 79, 2), (10, 77, 2), (12, 75, 4), (16, 75, 2), (18, 74, 2), (20, 72, 4), (24, 72, 8)];

/// One bar of the C major prelude's broken chord: its five notes, then
/// the top three again, the whole figure twice.
const fn prelude_bar(n: [u8; 5]) -> [(i64, u8, i64); 16] {
    let figure = [n[0], n[1], n[2], n[3], n[4], n[2], n[3], n[4]];
    let mut out = [(0, 0, 1); 16];
    let mut i = 0;
    while i < 16 {
        out[i] = (i as i64, figure[i % 8], 1);
        i += 1;
    }
    out
}
const PRELUDE_1: [(i64, u8, i64); 16] = prelude_bar([60, 64, 67, 72, 76]);
const PRELUDE_2: [(i64, u8, i64); 16] = prelude_bar([60, 62, 69, 74, 77]);
const PRELUDE_3: [(i64, u8, i64); 16] = prelude_bar([59, 62, 67, 74, 77]);

pub const PIECES: &[Piece] = &[
    Piece { title: "Ode to Joy", composer: "Beethoven", bpm: 100, phrases: &[ODE_A, ODE_B, ODE_A, ODE_C, ODE_D, ODE_E, ODE_A, ODE_C] },
    Piece { title: "Twinkle, Twinkle", composer: "Mozart", bpm: 100, phrases: &[TWINKLE_A, TWINKLE_B, TWINKLE_C, TWINKLE_C, TWINKLE_A, TWINKLE_B] },
    Piece { title: "Canon in D", composer: "Pachelbel", bpm: 70, phrases: &[CANON_A, CANON_B, CANON_C, CANON_D] },
    Piece { title: "Symphony No. 5", composer: "Beethoven", bpm: 100, phrases: &[FIFTH_A] },
    Piece { title: "Eine kleine Nachtmusik", composer: "Mozart", bpm: 100, phrases: &[NACHT_A, NACHT_B] },
    Piece { title: "Minuet in G", composer: "Petzold (Bach notebook)", bpm: 110, phrases: &[MINUET_A, MINUET_B, MINUET_C, MINUET_D] },
    Piece { title: "In the Hall of the Mountain King", composer: "Grieg", bpm: 100, phrases: &[KING_A, KING_B] },
    Piece { title: "Symphony No. 40", composer: "Mozart", bpm: 100, phrases: &[FORTY_A, FORTY_B] },
    Piece { title: "Für Elise", composer: "Beethoven", bpm: 80, phrases: &[ELISE_A, ELISE_B] },
    Piece { title: "Prelude in C", composer: "Bach", bpm: 70, phrases: &[&PRELUDE_1, &PRELUDE_2, &PRELUDE_3, &PRELUDE_1] },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phrase_fits_two_bars_in_order_and_in_range() {
        for piece in PIECES {
            assert!(!piece.phrases.is_empty(), "{}", piece.title);
            for (i, phrase) in piece.phrases.iter().enumerate() {
                assert!(!phrase.is_empty());
                assert!(phrase.windows(2).all(|w| w[0].0 < w[1].0), "{} phrase {i}: out of order", piece.title);
                assert!(phrase.iter().all(|n| n.0 + n.2 <= 32 && n.2 > 0), "{} phrase {i}: past two bars", piece.title);
                assert!(phrase.iter().all(|n| (48..=84).contains(&n.1)), "{} phrase {i}: out of range", piece.title);
                let ex = piece.exercise(i);
                assert!((1..=2).contains(&ex.bars));
            }
        }
    }

    #[test]
    fn the_tunes_are_the_tunes() {
        // Ode to Joy: E E F G G F E D.
        let ode: Vec<u8> = PIECES[0].phrases[0].iter().map(|n| n.1).collect();
        assert_eq!(ode, [64, 64, 65, 67, 67, 65, 64, 62]);
        // Für Elise begins E D# E D# E B D C A.
        let elise: Vec<u8> = PIECES.iter().find(|p| p.title == "Für Elise").unwrap().phrases[0].iter().take(9).map(|n| n.1).collect();
        assert_eq!(elise, [76, 75, 76, 75, 76, 71, 74, 72, 69]);
        // The prelude's first bar: C E G C E, G C E.
        assert_eq!(PRELUDE_1.iter().take(8).map(|n| n.1).collect::<Vec<_>>(), [60, 64, 67, 72, 76, 67, 72, 76]);
    }
}
