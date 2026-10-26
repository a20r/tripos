# Tripos

A Blade Runner themed SET card game, written in Rust and compiled to
WebAssembly. Static site on GitHub Pages, built for touch: a phone for solo
play, an iPad flat on the table for a group.

## Play

Every card has four attributes with three values each: number (one, two,
three), shape (diamond, pill, squiggle), colour, and shading (solid, striped,
open). Three cards are a set when each attribute is all the same or all
different across the three. Tap three cards to claim a set.

Three modes, chosen under **OPTIONS** at the bottom of the screen (`O` opens it):

- **Solo**: race the clock through the whole deck. A wrong pick adds 10
  seconds, `HINT` reveals one card of a set for 15 seconds (press again for a
  second card), and `DEAL 3` adds cards when no set is out (15 seconds if a
  set was there after all). The best time is kept.
- **Party**: two to four players around a tablet. Each player has a bar on
  their edge of the screen; bars on the far side are flipped so people across
  the table can read them. Tap your bar to call a set, then pick three cards
  within seven seconds. Right is +1, wrong or late is -1. The deck runs out
  and the high score wins.
- **Daily**: one deck order per calendar day, the same for everyone, seeded
  from the date. Your best time for the day and your streak of finished days
  are kept, and `SHARE` copies a one-line result.

- **Online**: two devices, one deck, with live voice. One player hosts a
  room and shares the five-letter code or the link; the other joins. Moves
  travel over a WebRTC data channel and each side sends its microphone, so you
  can talk while you play. First to tap a set takes it (the host settles
  races), a wrong pick costs a point, and either side can ask for three more
  cards when no set is out. The two browsers find each other through
  [ntfy.sh](https://ntfy.sh), a public pub/sub service, using the room code as
  the topic; no account or server of your own is needed, and `NTFY` in
  `src/net.rs` points at any other ntfy server. Only STUN is used, so a
  connection can fail between two strict NATs. Voice needs microphone
  permission; without it the game still plays and you still hear the other
  side. `?join=CODE` on the page URL opens the lobby with the code filled in,
  and `?signal=local` swaps the signalling for a same-browser channel, which
  the tests use.

- **Speed**: a drill. One triple at a time; swipe right if it is a set, left
  if it is not (or use the buttons, or the arrow keys). A round is twenty
  triples against the clock and a wrong swipe adds two seconds. Beating your
  best time rains confetti; falling short bleeds red down the screen. Half the
  triples are sets and most of the rest are near misses, where three of the
  four attributes follow the rule. Every answer is timed, and the profile
  gains a Speed Drill section: accuracy, median and 90th percentile reaction
  time, pace (correct answers per ten seconds), how often you swipe sets
  away, how often near misses fool you, and which kinds of set you spot
  quickest and slowest. A card pile under those verdicts opens a full-screen
  review of every wrong swipe, newest first and grouped by round, one triple
  per row with the reason it was or was not a set.

Three themes, also under **OPTIONS** (`T` cycles them):

- **Neon**: the Blade Runner look, pink, cyan and violet on near-black.
- **Classic**: a clean card-table look, red, green and purple on white.
- **Night**: an ultra-dark mode for bedtime. Pure black, dim warm tones, no
  glow or flicker.

The shuffle button next to `DEAL 3` rearranges the cards on the table, which
can make a hidden set easier to spot. It is free in every mode.

The profile button (bar chart, `P`) flips the table over to show how you
play across every Solo and Daily deck: for each kind of set (how many
attributes differ, all one colour or mixed, and the same for shape, count and
shading) how often you take it when it is on the table, which kinds you lean
toward, which you tend to miss, and which you find fastest. A set counts as
passed over when it was out and you took a different one, or you dealt while
it was out. A radar at the top of the report has one axis per kind of set
(same or mixed count, shape, colour and shading) with your table take rate
and your drill accuracy drawn over it as two shapes. The profile is kept on
the device and can be reset from the panel. The report follows the mode: in
Speed it shows the drill, everywhere else the table, and the reset button
clears only the part on show.

The choice of theme is remembered and applied before the first paint. Games in
progress are saved too.

Keys for desktop play: letters on the `qwerty` rows pick cards, `H` hint,
`D` deal, `N` new game, `R` rules, `1` to `4` call a set in Party, `Esc`
clears the selection.

## Offline

The site is a progressive web app. A service worker precaches the page, the
stylesheet, the fonts, the icons and the WebAssembly module on the first
online visit, so later visits open and play with no network at all, including
from a home screen icon. Online mode still needs a connection for the other
player. A new deploy is picked up on the next online visit and takes effect
the visit after that.

## Layout

- `engine/`: the rules (`is_set`, `third`, `find_sets`), the deck and table
  (`Game`, twelve cards topped up after each set, three more only on request),
  and daily seeding as a pure Rust crate with no dependencies
  (`cargo test -p tripos-engine`).
- `src/`: the browser game: state, persistence, SVG card faces, rendering and
  input through `web-sys`, compiled to WebAssembly with `wasm-bindgen`.
- `web/`: the static page, stylesheet and icon.
- `build.sh`: builds the module and assembles `dist/`.
- `.github/workflows/pages.yml`: tests, builds and deploys `dist/` to GitHub
  Pages on every push to `main`.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version "$(cargo metadata --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="wasm-bindgen"))')"
cargo test --workspace
./build.sh            # or ./build.sh debug
```

Then serve `dist/` with any static file server, for example:

```sh
cd dist && python3 -m http.server 8000
```

The server must send `.wasm` as `application/wasm` for streaming compilation;
GitHub Pages and Python's server both do.

## Hosting on GitHub Pages

1. In the repository settings open **Pages**.
2. Under **Build and deployment**, set **Source** to **GitHub Actions**.
3. Push to `main`. The workflow publishes the site at
   `https://<user>.github.io/tripos/`.
