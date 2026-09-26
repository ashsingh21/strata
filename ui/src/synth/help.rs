//! Carve's built-in sound-design guide: a toggleable overlay (the "?"
//! button in the device header) with one tab per stage of the signal
//! path. Every explanation is written against what `engine::synth`
//! actually does (ranges, what feeds what), not generic synth lore - and
//! the last tab loads a Deep Rave Bass patch and walks through it one
//! control at a time, so each idea can be heard, not just read.

use vizia::prelude::*;

use crate::tokens;

use super::state::SynthEvent;

/// One control or idea: its name, what it does and what you hear, and an
/// optional practical note (empty for none) on how it applies to a bass.
struct Entry {
    term: &'static str,
    body: &'static str,
    tip: &'static str,
}

struct Page {
    tab: &'static str,
    title: &'static str,
    intro: &'static str,
    entries: &'static [Entry],
}

const fn e(term: &'static str, body: &'static str, tip: &'static str) -> Entry {
    Entry { term, body, tip }
}

const PAGES: &[Page] = &[
    Page {
        tab: "Start",
        title: "How Carve makes a sound",
        intro: "Carve is a subtractive synth: it starts with a harmonically rich raw tone, then carves away what you don't want. \
                The signal runs left to right, exactly as the header says: Osc \u{2192} Mix \u{2192} Filter \u{2192} Amp \u{2192} Out. \
                Oscillators make the raw tone, Mix sets how much of each source you hear, the Filter removes brightness, \
                and the Amp envelope shapes loudness over time. The envelopes and LFOs don't make sound themselves - they move other knobs automatically.",
        entries: &[
            e("Harmonics - the key idea",
              "Every pitched sound is a stack of sine waves: the fundamental (the note you hear) plus harmonics at 2\u{00d7}, 3\u{00d7}, 4\u{00d7}... its frequency. \
               The waveform you pick decides which harmonics are there and how loud. More harmonics = brighter, buzzier; fewer = rounder, darker. \
               The filter then removes harmonics above its cutoff. Almost every knob here is really about harmonics.",
              ""),
            e("How to use this guide",
              "Hold a note and turn one knob at a time while you listen. Change one thing, hear it, put it back. \
               Low sounds need headphones or real speakers - laptop speakers can't reproduce much below ~150 Hz, so a sub-bass can sound like nothing at all on them.",
              ""),
            e("Playing notes",
              "Click keys on the on-screen keyboard (press and hold), or type: Z X C V B N M , . / are white keys, S D G H J L ; the black keys between them. \
               Z is C4 by default; - and + shift the whole typing keyboard down/up an octave (shown as \"Oct\").",
              "For bass, press - once so Z plays C3."),
            e("Mono / Poly and Voices",
              "Poly lets several notes sound at once (up to Voices of them) - for chords and pads. \
               Mono plays one note at a time: a new note takes over from the old one, and if you press it before letting go of the previous one (legato), \
               the envelopes don't restart and Glide slides the pitch across.",
              "Basses are almost always Mono: overlapping low notes turn into mud, and Mono is what makes Glide work."),
            e("Volume", "The synth's final output level (the meter next to it shows the result).", ""),
        ],
    },
    Page {
        tab: "Waves",
        title: "Waveforms: the raw colour of the sound",
        intro: "The sin / tri / saw / sq buttons on each oscillator pick its basic shape. The shape decides which harmonics exist, \
                and that is most of what makes a synth sound 'soft', 'hollow' or 'buzzy' before any filtering.",
        entries: &[
            e("sin - Sine",
              "One pure frequency, no harmonics at all. Smooth, round, almost invisible in a mix above the low end - it has nothing for a filter to remove.",
              "The sound of a clean sub-bass. You feel it more than hear it."),
            e("tri - Triangle",
              "Only odd harmonics (3\u{00d7}, 5\u{00d7}, 7\u{00d7}...) and they fade out quickly. A sine with a little edge - soft, flute-like, a touch hollow.",
              "Good for a deep bass that should stay gentle and audible on small speakers."),
            e("saw - Sawtooth",
              "Every harmonic, falling off gently - the richest, brightest, buzziest shape. Sounds like brass or bowed strings raw; \
               it's the default starting point for subtractive synthesis because it gives the filter the most to work with.",
              "The backbone of rave, techno and reese basses."),
            e("sq - Square / Pulse",
              "Odd harmonics only, falling off gently: hollow, woody, reedy (think clarinet, or 8-bit game music). \
               Its width can be changed (see PW) - narrowing it thins the sound out and makes it nasal.",
              "Punchy and hollow; sits well an octave above a sine sub."),
            e("Shape (Osc 1) / PW (Osc 2)",
              "The same kind of control on both oscillators - it bends the waveform. At 0% every shape is pure. Turning it up: \
               sine gains an added 2nd harmonic (fatter, slightly organ-like); triangle leans over toward a saw (brighter); \
               saw rounds off toward a triangle (softer); square changes its pulse width - 50% is the classic hollow square, \
               towards 5% or 95% it becomes a thin, nasal pulse.",
              "On a saw bass, keep it near 0% for maximum bite."),
        ],
    },
    Page {
        tab: "Oscs + Mix",
        title: "Oscillators and the Mix",
        intro: "Two oscillators, a sub oscillator and a noise source. The magic of analog-style sounds mostly comes from how the two \
                oscillators interact: slightly out of tune, locked together (Sync), or one wobbling the other (FM).",
        entries: &[
            e("Octave",
              "Each oscillator's pitch in whole octaves relative to the key you play (shown as \"-1 oct\"). -1 means one octave lower than the key.",
              ""),
            e("Tune (Osc 1) / Detune (Osc 2)",
              "Fine pitch offset in cents (100 cents = 1 semitone). Tune is \u{00b1}100 ct, Detune \u{00b1}50 ct. \
               Two oscillators a few cents apart drift in and out of phase, so their sum pulses and swirls - called 'beating'. \
               ~3-8 ct: gentle warmth and width. ~10-20 ct: an obvious slow churn. 30+ ct: fast, sour, seasick.",
              "Two saws detuned by ~10-15 ct is the famous 'reese' bass (the moving, growling bass of jungle, DnB and rave)."),
            e("Drift (Osc 1)",
              "A slow, random pitch wander of up to \u{00b1}15 cents, re-randomising every couple of seconds, separately for each voice. \
               Imitates old analog oscillators that never stay perfectly in tune. Subtle: it adds life and makes repeats sound less robotic.",
              "10-25% adds warmth; above ~50% it starts sounding out of tune."),
            e("FM (Osc 2)",
              "Frequency modulation: Osc 1's waveform speeds up and slows down Osc 2's pitch hundreds of times a second. \
               That's too fast to hear as wobble - instead it creates new harmonics. \
               Small amounts add grit and growl; large amounts turn metallic, bell-like or clangy, especially if the two octaves aren't related.",
              "5-20% gives a bass teeth without making it brighter in an obvious way."),
            e("SYNC (Osc 2)",
              "Hard sync: every time Osc 1 finishes a cycle, Osc 2 is forced to restart its cycle. Osc 2's pitch then locks to Osc 1's, \
               and any difference between them turns into a change of tone instead of a change of pitch. \
               You only really hear it when Osc 2 is set higher than Osc 1 (e.g. Osc 2 an octave up) - then moving Osc 2's pitch gives the tearing, screaming 'sync sweep' sound.",
              "Sync kills detune beating (Osc 2 can't drift against Osc 1 while locked) - keep it off for a reese."),
            e("Mix: Osc 1 / Osc 2",
              "Each oscillator's level going into the filter, 0 dB (full) down to -60 dB (effectively off).",
              ""),
            e("Mix: Sub",
              "A pure sine one octave below Osc 1 (it follows Osc 1's Octave setting). It adds weight and depth without adding any buzz, \
               because a sine has no harmonics. Mostly felt in your chest; on laptop speakers you may not hear it at all.",
              "The 'deep' in deep bass. Turn it up and the low end gets heavier; the saws on top provide the part you can actually hear."),
            e("Mix: Noise",
              "White noise (every frequency at once - a hiss). It passes through the filter and amp envelope like the oscillators do, \
               so with a low cutoff it becomes a dark rumble or breath rather than hiss.",
              "A touch (-35 to -25 dB) adds grit and air to the attack; keep it off for a clean deep bass."),
        ],
    },
    Page {
        tab: "Filter",
        title: "The Filter: carving the brightness",
        intro: "The filter removes frequencies. It's the most expressive part of a subtractive synth - most of the 'wow', 'squelch' and 'pluck' \
                you hear in electronic music is the filter's cutoff moving. The display above the knobs draws the curve it's applying.",
        entries: &[
            e("Type: LP 24 / LP 12 / BP / HP",
              "LP (low-pass) keeps everything below the cutoff and removes what's above - the normal choice. \
               LP 24 cuts steeply (24 dB per octave): darker, rounder, more 'closed'. LP 12 cuts gently: more buzz leaks through, brighter and rawer. \
               BP (band-pass) keeps only a band around the cutoff - thin, nasal, telephone-like. HP (high-pass) removes the lows - thin, never for a bass.",
              "LP 24 for a deep, round bass; LP 12 if you want it to cut through more."),
            e("Cutoff",
              "The frequency where the filter starts removing things (20 Hz - 20 kHz). This is the brightness knob: turn it down and the sound gets darker, \
               muffled, further away; up and it gets brighter, buzzier, closer. With a low-pass, it can't remove the fundamental without the note fading too.",
              "Deep basses live with the cutoff low (roughly 100-400 Hz) and let the filter envelope open it on each note."),
            e("Reso (Resonance)",
              "Boosts the frequencies right at the cutoff, creating a peak. Low: smooth. Medium: a vocal, 'wah' edge. \
               High: the filter rings and whistles at the cutoff frequency. When the cutoff moves, the peak sweeps through the harmonics - \
               that's the 'squelch' of acid (TB-303) basslines.",
              "20-40% adds focus; 70%+ is acid territory, and the peak can start to overpower the bass body."),
            e("Drive",
              "Turns the signal up (0 to +24 dB) into a soft clipper before it reaches the filter. The loud peaks get rounded off, which \
               adds new harmonics (warmth at low settings, grit and distortion at high) and squashes the dynamics so it sounds denser and louder. \
               Because it happens before the filter, the filter then tames the fizziest part of the distortion.",
              "6-12 dB is the classic 'fat, slightly overdriven' rave bass."),
            e("Env (envelope amount)",
              "How far the Filter Env moves the cutoff, in octaves (-4 to +4). +3 oct means: at the envelope's peak the cutoff is 3 octaves (8\u{00d7}) above the Cutoff knob, \
               then it falls back as the envelope decays. Negative values move it down instead. At 0 the filter envelope does nothing at all.",
              "This plus a short Filter Env decay is what gives each bass note its 'bwow' punch."),
            e("Key trk (key tracking)",
              "How much the cutoff follows the note you play, measured from C4. At 100% the cutoff moves one octave for every octave you play, \
               so every note has the same brightness. At 0% the cutoff stays put, so high notes sound duller (more of their harmonics are above the cutoff) \
               and low notes brighter.",
              "~50% keeps low notes deep while stopping higher notes going dull."),
            e("LFO on Cutoff (the ring on the knob)",
              "When an LFO targets Cutoff, the coloured ring on the Cutoff knob and the shaded band in the display show how far it's sweeping.",
              ""),
        ],
    },
    Page {
        tab: "Envelopes",
        title: "Filter Env vs Amp Env",
        intro: "An envelope is an automatic hand on a knob that moves every time you press a key. Both envelopes have the same four stages (ADSR) - \
                the difference is only which knob they move. The Amp Env moves loudness. The Filter Env moves the filter's cutoff (by the Filter's Env amount). \
                So the Amp Env decides how LOUD the note is over time; the Filter Env decides how BRIGHT it is over time.",
        entries: &[
            e("A - Attack", "How long it takes to rise from zero to the peak after you press a key (1 ms - 2 s). Short = instant, percussive. Long = swelling, fading in.", ""),
            e("D - Decay", "How long it takes to fall from the peak down to the sustain level (1 ms - 2 s).", ""),
            e("S - Sustain", "The level it holds for as long as you keep the key down (0-100%). It's a level, not a time.", ""),
            e("R - Release", "How long it takes to fade back to zero after you let go of the key (1 ms - 2 s).", ""),
            e("Amp Env - loudness over time",
              "The volume of the note. A short attack, some decay and high sustain = a note that starts with a punch and holds. \
               Sustain 0% with a short decay = a note that dies away on its own (a pluck or stab), even while you hold the key. \
               Long release = notes ring on after you let go (can smear a bassline).",
              "Bass: attack 1-5 ms, sustain high, release short (50-150 ms) so notes stop cleanly."),
            e("Filter Env - brightness over time",
              "The same shape, but moving the cutoff instead of the volume. Fast attack, short decay, low sustain gives the classic 'pluck': \
               each note starts bright and quickly darkens while staying just as loud. A slow attack makes the note 'open up' - a swell or wah. \
               Remember it only works if the Filter's Env knob isn't at 0, and the higher that knob, the bigger the movement.",
              ""),
            e("Hearing the difference",
              "Try: Amp Env decay short with sustain 0 - the note gets QUIETER and disappears. Now put the Amp sustain back up and instead set Filter Env decay short, \
               sustain 0 - the note stays LOUD but gets DARKER. Same shape, different target. Most good basses use both: the amp holds the note, the filter gives it motion.",
              ""),
            e("In Mono mode",
              "Playing legato (pressing the next key before releasing the previous one) does not restart the envelopes - the new note glides in without a fresh 'bwow'. \
               Detached notes each get a full envelope. That's how bass players get accents: play some notes detached, some connected.",
              ""),
        ],
    },
    Page {
        tab: "Movement",
        title: "LFOs and Glide",
        intro: "An LFO (low-frequency oscillator) is an oscillator too slow to hear as a pitch - instead it wiggles another knob back and forth, continuously, \
                whether or not you're playing. Glide makes the pitch slide between notes.",
        entries: &[
            e("Rate", "How fast the LFO cycles, 0.05 Hz (one cycle every 20 s) to 20 Hz (a fast flutter). The readout shows Hz.", ""),
            e("Depth",
              "How far it moves its target. On Cutoff, 100% sweeps \u{00b1}2 octaves. On Pitch, 100% is \u{00b1}50 cents (a quarter tone) of vibrato.",
              ""),
            e("Target button (\u{2192} Cutoff / \u{2192} Pitch)",
              "Click to switch what the LFO moves. Cutoff: a rhythmic 'wah' or, at 1-6 Hz with high depth, the dubstep-style 'wobble'. \
               Pitch: vibrato - a few Hz at low depth sounds like a singer; slow and shallow sounds like a warped tape.",
              "On a deep bass keep pitch vibrato at 0 - a wobbling pitch in the low end sounds out of tune. A tiny, slow cutoff LFO adds life."),
            e("Routing an LFO",
              "Press an LFO pill (LFO 1 / LFO 2) and release it over any knob that lights up: Cutoff, Resonance, Tune (pitch) or Pulse width. \
               That LFO now moves that knob, and the knob grows a blue ring showing how far it swings. The target button under the LFO's knobs cycles through the same four.",
              "Pulse width at slow rate and medium depth is classic PWM: a square wave that shimmers and thickens."),
            e("Sync (LFO)", "Meant to lock the LFO to the song tempo. Display only for now - the LFO always runs at the Rate shown.", ""),
            e("Unison: Voices, Detune, Width",
              "Plays up to 4 copies of both oscillators per note, spread in pitch by Detune (the cents between the outermost copies) and across the stereo field by Width. \
               This is the 'supersaw' effect: thick, wide and chorused. It costs CPU per copy.",
              "Leave Unison off (Voices: Off) for a deep bass - width and detune in the low end smear it. Use it on leads, pads and stabs."),
            e("Effects: Chorus and Reverb",
              "After the voices, the sound runs through a stereo chorus (two gently moving delays, one per side - Depth sets how far they move) and a reverb (a simulated room - Size sets how long it rings). \
               Each knob is its wet mix: 0% is off. A limiter at the very end stops the output from ever clipping, however loud a chord gets.",
              "Small amounts go a long way: 20-30% chorus, 10-20% reverb."),
            e("Glide (portamento)",
              "In Mono mode, the time the pitch takes to slide to the next note (1-500 ms). It only happens when notes overlap - hold one key, press the next, then release the first. \
               Notes played detached jump straight to pitch. Glide does nothing in Poly.",
              "40-100 ms is the rubbery slide in acid and rave basslines; 200+ ms is a slow, dramatic swoop."),
        ],
    },
    Page {
        tab: "\u{2605} Rave bass",
        title: "Recipe: Deep Rave Bass",
        intro: "Load the patch, press - once (so Z plays C3), hold Z, and work down the list. Each step changes one control so you can hear what it contributes, \
                then puts it back. Reload the patch any time to start over.",
        entries: &[
            e("What's in it",
              "Mono, 70 ms glide. Two saws an octave down, Osc 2 detuned +12 ct (reese beating), Drift 20%. Sub at -3 dB. \
               LP 24 at 220 Hz, Reso 35%, Drive +9 dB, Env +3 oct, Key trk 50%. Filter Env: 1 ms / 220 ms / 15% / 150 ms. \
               Amp Env: 2 ms / 400 ms / 85% / 90 ms. LFO 1 barely touching Cutoff, LFO 2 vibrato off.",
              ""),
            e("1. The floor: Sub alone",
              "Turn Osc 1 and Osc 2 in the Mix down to -60 dB. What's left is the pure sine sub - deep and smooth, and nearly silent on small speakers. \
               That's the weight. Put both back (-4 and -5 dB).",
              ""),
            e("2. The buzz: the saws",
              "Now turn Sub to -60 dB. You hear only the saws: gritty and audible anywhere, but thin in the low end. Deep bass = sub for weight + saw for presence. Sub back to -3 dB.",
              ""),
            e("3. The movement: Detune",
              "Set Osc 2 Detune to 0 - the churning stops; it's static and flat. Try +30 ct - fast and sour. Back to about +12 ct for the slow, rolling reese motion.",
              ""),
            e("4. Sync kills the beating",
              "Click Osc 2's SYNC on: the swirling disappears because Osc 2 is now locked to Osc 1. Click it off again.",
              ""),
            e("5. Brightness: Cutoff",
              "Sweep Cutoff from its lowest to ~3 kHz while holding the note: from a muffled rumble to a raw buzz. Settle back around 200-250 Hz.",
              ""),
            e("6. The punch: Filter Env",
              "Set the Filter's Env knob to 0: every note is a static dark tone. Back to +3 oct: each note starts bright and closes - 'bwow'. \
               Now play repeated notes and change Filter Env Decay: 60 ms is tight and clicky, 800 ms is a slow 'wah'.",
              ""),
            e("7. Fatness: Drive",
              "Drive to 0 dB: cleaner, smaller. +20 dB: distorted and aggressive. The sweet spot for a fat rave bass is roughly +6 to +12 dB.",
              ""),
            e("8. Acid: Resonance",
              "Turn Reso up to 75-85% and play a few notes: the filter envelope now sweeps a whistling peak through the sound - the acid squelch. Back to ~35% for deep.",
              ""),
            e("9. The slide: Glide",
              "Hold Z, press B, release Z: the pitch slides up a fifth. Play them separately and it jumps. Turn Glide up to 300 ms for a dramatic swoop.",
              ""),
            e("10. Seasoning: Drift, FM, Noise",
              "Drift to 60%: noticeably wobbly, vintage-sounding. FM to ~15%: extra growl in the tone. Noise to -30 dB: gritty air on each note. Pick what you like.",
              ""),
            e("Make it yours",
              "Darker and heavier: Cutoff down, Sub up, Env down. More aggressive: Drive up, LP 12, FM up. Rolling reese: Detune 15-20 ct, longer Filter decay. \
               Classic stab: Amp sustain 0%, Amp decay 200 ms.",
              ""),
        ],
    },
];

const RAVE_BASS_TAB: usize = PAGES.len() - 1;

fn entry_view(cx: &mut Context, entry: &'static Entry) {
    VStack::new(cx, move |cx| {
        Label::new(cx, entry.term).class("guide-term");
        Label::new(cx, entry.body).class("guide-body").text_wrap(true).width(Stretch(1.0));
        if !entry.tip.is_empty() {
            Label::new(cx, format!("\u{2192} {}", entry.tip))
                .class("guide-tip")
                .text_wrap(true)
                .width(Stretch(1.0));
        }
    })
    .gap(Pixels(2.0))
    .width(Stretch(1.0))
    .height(Auto);
}

fn load_rave_bass(cx: &mut EventContext) {
    cx.emit(SynthEvent::LoadPreset(shared::synth::deep_rave_bass));
}

pub fn help_overlay(cx: &mut Context, open: Signal<bool>) {
    let tab = Signal::new(0usize);

    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, "Carve guide").class("control");
            HStack::new(cx, move |cx| {
                for (i, page) in PAGES.iter().enumerate() {
                    Button::new(cx, |cx| Label::new(cx, page.tab))
                        .class("synth-seg-btn")
                        .toggle_class("is-on", tab.map(move |t| *t == i))
                        .on_press(move |_| tab.set(i));
                }
            })
            .class("synth-seg")
            .size(Auto);
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Button::new(cx, |cx| Label::new(cx, "Close"))
                .class("btn")
                .class("sm")
                .on_press(|cx| cx.emit(SynthEvent::ToggleHelp));
        })
        .gap(Pixels(tokens::SPACE_3))
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Auto);

        // Rebuilt from scratch whenever the guide opens or the tab changes,
        // rather than toggling each page's `display`: text inside a view
        // going from `display: none` to shown never got laid out, so every
        // tab but the first (and all of them, once closed and reopened)
        // came up blank.
        Binding::new(cx, open, move |cx| {
            Binding::new(cx, tab, move |cx| {
                if !open.get() {
                    return;
                }
                let i = tab.get();
                let page = &PAGES[i];
                ScrollView::new(cx, move |cx| {
                    VStack::new(cx, move |cx| {
                        Label::new(cx, page.title).class("guide-h");
                        Label::new(cx, page.intro).class("guide-body").text_wrap(true).width(Stretch(1.0));
                        if i == RAVE_BASS_TAB {
                            Button::new(cx, |cx| Label::new(cx, "Load Deep Rave Bass"))
                                .class("btn")
                                .on_press(load_rave_bass);
                        }
                        for entry in page.entries {
                            entry_view(cx, entry);
                        }
                    })
                    .gap(Pixels(tokens::SPACE_3))
                    .padding_right(Pixels(tokens::SPACE_3))
                    .width(Stretch(1.0))
                    .height(Auto);
                })
                // Scroll with the wheel only: the app opts out of Vizia's default
                // theme, so its scrollbars come unstyled - absolutely positioned
                // over the whole view, hiding everything in it.
                .show_horizontal_scrollbar(false)
                .show_vertical_scrollbar(false)
                .width(Stretch(1.0))
                .height(Stretch(1.0));
            });
        });
    })
    .class("panel")
    .class("synth-help-panel")
    .toggle_class("hidden", open.map(|b| !*b))
    .gap(Pixels(tokens::SPACE_3))
    .padding(Pixels(tokens::SPACE_3))
    .position_type(PositionType::Absolute)
    .top(Pixels(0.0))
    .left(Pixels(0.0))
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}
